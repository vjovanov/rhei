//! The durable event log: `runtime/events.jsonl`.
//!
//! Every non-dry run writes it, whichever frontend is selected, so a separate
//! process can follow a run it did not start. It is truncated at run start —
//! one file is one run, which is what lets `seq` begin at 1 and a replay be
//! bounded.
// §FS-rhei-run-json.3 §FS-rhei-run-headless.5

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::rhei_tui::event::{EventSink, RunEvent};
use crate::rhei_tui::event_json;

/// Name of the log inside a workspace's `runtime/` directory.
pub const EVENT_LOG_NAME: &str = "events.jsonl";

/// Path of the event log for a workspace root.
pub fn event_log_path(workspace_root: impl AsRef<Path>) -> PathBuf {
    workspace_root.as_ref().join("runtime").join(EVENT_LOG_NAME)
}

/// Append-only writer for the run's JSONL event log.
/// Flushed after every record so a follower sees each line as it lands. A write
/// failure warns once on stderr and disables the sink: a run that cannot be
/// watched is still a run that works.
// §FS-rhei-run-headless.8
pub struct EventLogSink {
    inner: Mutex<Option<File>>,
    seq: AtomicU64,
    path: PathBuf,
    /// Kept so records name their paths the way the journal does:
    /// workspace-relative inside the workspace. §FS-rhei-run-json.2.1
    workspace_root: PathBuf,
}

impl EventLogSink {
    /// Create (truncating any previous run's) `<workspace>/runtime/events.jsonl`.
    pub fn create(workspace_root: impl AsRef<Path>) -> std::io::Result<Self> {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let path = event_log_path(&workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(&path)?;
        Ok(Self { inner: Mutex::new(Some(file)), seq: AtomicU64::new(0), path, workspace_root })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Sequence number of the last record written.
    pub fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

impl EventSink for EventLogSink {
    fn emit(&self, event: RunEvent) {
        if !event_json::is_structural(&event) {
            return;
        }
        let at = event_json::event_wall_clock(&event).unwrap_or_else(SystemTime::now);
        // The lock spans the sequence bump so records cannot interleave out of
        // order: two workers finishing at once must not swap `seq` and line.
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let Some(file) = guard.as_mut() else {
            return;
        };
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let line =
            format!("{}\n", event_json::encode(Some(seq), &event, at, Some(&self.workspace_root)));
        if let Err(err) = file.write_all(line.as_bytes()).and_then(|()| file.flush()) {
            eprintln!("warning: event log write failed ({}): {err}", self.path.display());
            *guard = None;
        }
    }
}

/// A follower of a run's event log.
/// Reads whole lines from a byte offset and remembers where it stopped, so a
/// partially written final line is re-read on the next poll rather than
/// decoded torn. This is how an attached surface tracks a live run
// §FS-rhei-run-headless.5.
pub struct EventLogReader {
    path: PathBuf,
    offset: u64,
    last_seq: u64,
}

impl EventLogReader {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), offset: 0, last_seq: 0 }
    }

    /// Sequence number of the last record this reader returned.
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Read every complete record appended since the previous call, each with
    /// the sequence number and timestamp it was written under.
    /// Undecodable lines are skipped, not fatal: a record kind from a newer
    /// `rhei` costs one missing update, never the surface.
    // §FS-rhei-run-json.2
    pub fn poll(&mut self) -> Vec<event_json::DecodedRecord> {
        let Ok(file) = File::open(&self.path) else {
            return Vec::new();
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // The file shrank: a new run truncated it. Start over rather than
            // read from a stale offset into the middle of a fresh record.
            self.offset = 0;
            self.last_seq = 0;
        }
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut events = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(read) => {
                    if !line.ends_with('\n') {
                        // The run is mid-write. Leave the offset before this
                        // fragment so the whole line is read next time.
                        break;
                    }
                    self.offset += read as u64;
                    if let Some(record) = event_json::decode(&line) {
                        if let Some(seq) = record.seq {
                            self.last_seq = seq;
                        }
                        events.push(record);
                    }
                }
                Err(_) => break,
            }
        }
        events
    }
}

#[cfg(test)]
mod tests;
