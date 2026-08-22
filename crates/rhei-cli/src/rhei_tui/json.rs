//! The `--json` frontend: the run as a JSONL stream on stdout.
//!
//! Nothing but records reaches stdout, ever. Engine prose that the plain
//! frontend would print arrives as `message` records instead, and errors go to
//! stderr in the envelope of  — so a consumer parses one shape
//! from the first byte to the last.
// §FS-rhei-errors.5 §FS-rhei-run-json.1

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::rhei_tui::event::{EventSink, RunEvent};
use crate::rhei_tui::event_json;

/// Stdout writer for the JSONL event stream.
pub struct JsonSink {
    seq: AtomicU64,
    /// Serializes the write so two workers' records cannot interleave mid-line.
    out: Mutex<()>,
    /// Whether `agent_output` records are inlined (`--json-agent-output`)
    /// instead of left to the per-task logs. §FS-rhei-run-json.2.3
    agent_output: bool,
    /// The run's workspace, so a record's paths are named as the durable log
    /// and the journal name them. §FS-rhei-run-json.2.1
    workspace_root: PathBuf,
}

impl JsonSink {
    pub fn new(agent_output: bool, workspace_root: impl AsRef<Path>) -> Self {
        Self {
            seq: AtomicU64::new(0),
            out: Mutex::new(()),
            agent_output,
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }
}

impl EventSink for JsonSink {
    fn emit(&self, event: RunEvent) {
        if !self.agent_output && !event_json::is_structural(&event) {
            return;
        }
        let at = event_json::event_wall_clock(&event).unwrap_or_else(SystemTime::now);
        let guard = match self.out.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        // Structural records alone carry — and burn — a sequence number, so
        // the numbering on stdout is identical to the one in
        // `runtime/events.jsonl`, whose filter is not the same as this one.

        // §FS-rhei-run-json.2 §FS-rhei-run-json.2.3
        let seq =
            event_json::is_structural(&event).then(|| self.seq.fetch_add(1, Ordering::SeqCst) + 1);
        let mut stdout = std::io::stdout().lock();
        // A failed write is the reader going away, which the run's own
        // lost-output path already handles as a broken pipe on the way out.
        // Nothing useful can be said about it *here* — saying it on stderr
        // would be the only non-record byte this frontend ever produced.

        // §FS-rhei-run.3.2
        let _ =
            writeln!(stdout, "{}", event_json::encode(seq, &event, at, Some(&self.workspace_root)));
        let _ = stdout.flush();
        drop(guard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhei_tui::event::{AgentStream, MessageLevel};

    fn output_event() -> RunEvent {
        RunEvent::AgentOutput {
            slot: 0,
            task: "auth.1".to_string(),
            stream: AgentStream::Stdout,
            line: "hello".to_string(),
            wall_clock: SystemTime::now(),
        }
    }

    /// The sink writes to the process's stdout, which a unit test cannot
    /// capture, so what is asserted here is the filtering decision that governs
    /// *whether* a line is written. The stream shape itself is covered by the
    /// record contract tests and end-to-end by the CLI tests.
    #[test]
    fn agent_output_is_dropped_unless_it_was_asked_for() {
        let quiet = JsonSink::new(false, "/nowhere");
        assert!(!quiet.agent_output);
        quiet.emit(output_event());
        assert_eq!(quiet.seq.load(Ordering::SeqCst), 0, "a dropped event burns no sequence number");

        // Nor does a *written* one: `agent_output` is not a cursor point, so
        // it carries no `seq` and cannot desynchronise the structural
        // numbering from the durable log's. §FS-rhei-run-json.2.3
        let loud = JsonSink::new(true, "/nowhere");
        loud.emit(output_event());
        assert_eq!(loud.seq.load(Ordering::SeqCst), 0);
        loud.emit(RunEvent::Message { level: MessageLevel::Info, text: "structural".to_string() });
        assert_eq!(loud.seq.load(Ordering::SeqCst), 1, "structural records number from 1");
    }

    #[test]
    fn structural_events_are_always_written() {
        let sink = JsonSink::new(false, "/nowhere");
        sink.emit(RunEvent::Message { level: MessageLevel::Info, text: "hi".to_string() });
        assert_eq!(sink.seq.load(Ordering::SeqCst), 1);
    }
}
