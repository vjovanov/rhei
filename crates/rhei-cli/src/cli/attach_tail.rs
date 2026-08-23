// Live agent traffic for an attached surface.
//
// The run does not put agent output in its event log: the per-task log already
// holds the complete transcript, and duplicating it would make the log
// unbounded. So an attached surface reads the transcript where it lives,
// following the paths `slot_assigned` names.

// §FS-rhei-run-tui.1.2 §FS-rhei-run-headless.5 §FS-rhei-run-json.2.3

/// A per-task log this surface is following, and how far it has read.
struct TailedLog {
    task: String,
    slot: rhei_tui::Slot,
    path: PathBuf,
    offset: u64,
    /// Whether the first read must throw away everything up to the next
    /// newline. A backfill offset lands wherever `len - BACKFILL_BYTES` falls,
    /// which is mid-line, and emitting that tail as a whole line presents a
    /// fragment as something the agent wrote.
    starts_mid_line: bool,
}

/// Follows the per-task logs of a run's live slots and turns their new lines
/// into `AgentOutput` events, so an attached surface shows the same traffic a
/// driving one does.
#[derive(Default)]
pub(crate) struct AgentLogTailer {
    open: Vec<TailedLog>,
}

/// A tailed log's first read starts here rather than at byte 0: an operator who
/// attaches to a slot that has been running for an hour wants its recent
/// output, not an hour of replay into a 50-line buffer.
const BACKFILL_BYTES: u64 = 16 * 1024;

impl AgentLogTailer {
    /// Note a slot's log so its output is followed, resolving the path against
    /// the workspace when the event carried a relative one.
    pub(crate) fn follow(
        &mut self,
        workspace: &Path,
        task: &str,
        slot: rhei_tui::Slot,
        log_path: &Path,
    ) {
        let path =
            if log_path.is_absolute() { log_path.to_path_buf() } else { workspace.join(log_path) };
        let offset = fs::metadata(&path)
            .map(|meta| meta.len().saturating_sub(BACKFILL_BYTES))
            .unwrap_or(0);
        self.open.retain(|open| !(open.task == task && open.slot == slot));
        self.open.push(TailedLog {
            task: task.to_string(),
            slot,
            path,
            offset,
            starts_mid_line: offset > 0,
        });
    }

    /// Stop following a slot's log. Called on release, after one last read so a
    /// worker's final lines are not lost to the race between its last write and
    /// the event that says it exited.
    pub(crate) fn release(&mut self, task: &str, slot: rhei_tui::Slot) -> Vec<rhei_tui::RunEvent> {
        let Some(index) =
            self.open.iter().position(|open| open.task == task && open.slot == slot)
        else {
            return Vec::new();
        };
        let mut closing = self.open.remove(index);
        read_new_lines(&mut closing)
    }

    /// Every line appended to a followed log since the last poll.
    pub(crate) fn poll(&mut self) -> Vec<rhei_tui::RunEvent> {
        let mut events = Vec::new();
        for open in &mut self.open {
            events.extend(read_new_lines(open));
        }
        events
    }
}

/// Read whole lines from a log's current offset, leaving a partial trailing
/// line for the next poll.
fn read_new_lines(log: &mut TailedLog) -> Vec<rhei_tui::RunEvent> {
    let Ok(file) = fs::File::open(&log.path) else {
        return Vec::new();
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    if len < log.offset {
        // Rotated or replaced under us; re-read from the new start rather than
        // from an offset that now points into the middle of a line.
        log.offset = 0;
        log.starts_mid_line = false;
    }
    let mut reader = BufReader::new(file);
    if reader.seek(std::io::SeekFrom::Start(log.offset)).is_err() {
        return Vec::new();
    }
    let mut events = Vec::new();
    let mut line = String::new();
    if log.starts_mid_line {
        // Discard the fragment the backfill offset landed inside. If the rest
        // of that line has not been written yet there is nothing to discard
        // yet either, so leave the offset where it is and try again next poll.
        match reader.read_line(&mut line) {
            Ok(read) if line.ends_with('\n') => {
                log.offset += read as u64;
                log.starts_mid_line = false;
            }
            _ => return events,
        }
        line.clear();
    }
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(read) => {
                if !line.ends_with('\n') {
                    break;
                }
                log.offset += read as u64;
                events.push(rhei_tui::RunEvent::AgentOutput {
                    slot: log.slot,
                    task: log.task.clone(),
                    // The per-task log interleaves both streams into one file,
                    // so the split the live run makes is not recoverable here.
                    // Reporting it all as stdout is honest; guessing from the
                    // text would be a fabrication the run never made.
                    stream: rhei_tui::AgentStream::Stdout,
                    line: line.trim_end_matches(['\n', '\r']).to_string(),
                    wall_clock: std::time::SystemTime::now(),
                });
            }
            Err(_) => break,
        }
    }
    events
}

