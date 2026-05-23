// AR §7: the host side of the one mutation boundary. `RunInterveneSink` keeps a
// per-task registry of open agent-stdin writers, delivers `/intervene` messages
// to that stdin and nothing else, and records every delivery (and every failure
// reason) to a durable audit trail at `runtime/interventions.log`, mirroring
// successful messages into the task's agent transcript.
//
// There is no path from here to plan-state mutation: a delivery resolves a task,
// sends bytes to that task's child stdin, logs, and returns.

use std::sync::mpsc::{channel, Sender};

/// One registered running task: a channel to its stdin writer thread, the
/// shared handle to its durable agent log (so a mirror line cannot interleave
/// with the agent's own output), and identifying metadata for the audit trail.
struct InterveneTarget {
    stdin_tx: Sender<Vec<u8>>,
    log_file: Arc<Mutex<fs::File>>,
    slot: rhei_tui::Slot,
    state: String,
}

pub(crate) struct RunInterveneSink {
    runtime_dir: PathBuf,
    targets: Mutex<HashMap<String, InterveneTarget>>,
    audit: Mutex<()>,
}

impl RunInterveneSink {
    pub(crate) fn new(runtime_dir: PathBuf) -> Self {
        Self { runtime_dir, targets: Mutex::new(HashMap::new()), audit: Mutex::new(()) }
    }

    /// Register a running task's stdin for intervention. The provided
    /// `child_stdin` is moved into a dedicated writer thread that holds it open
    /// for the process lifetime and writes any delivered messages; the thread
    /// exits (closing stdin) when the task is unregistered.
    pub(crate) fn register(
        &self,
        task_id: &str,
        slot: rhei_tui::Slot,
        state: &str,
        log_file: Arc<Mutex<fs::File>>,
        child_stdin: std::process::ChildStdin,
    ) {
        let (tx, rx) = channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut stdin = child_stdin;
            while let Ok(bytes) = rx.recv() {
                if stdin.write_all(&bytes).is_err() {
                    break;
                }
                let _ = stdin.flush();
            }
            // Sender dropped (task unregistered) or write failed: drop stdin,
            // closing the pipe.
        });
        if let Ok(mut targets) = self.targets.lock() {
            targets.insert(
                task_id.to_string(),
                InterveneTarget { stdin_tx: tx, log_file, slot, state: state.to_string() },
            );
        }
    }

    /// Drop a task's registration once its agent exits. Dropping the stored
    /// sender ends the writer thread, which closes the child's stdin.
    pub(crate) fn unregister(&self, task_id: &str) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.remove(task_id);
        }
    }

    /// Append one line to the durable audit trail. Serialized through `audit`
    /// so concurrent deliveries do not interleave.
    fn audit(&self, task_id: &str, slot: Option<rhei_tui::Slot>, state: &str, message: &str, outcome: &str) {
        let _guard = self.audit.lock();
        let path = self.runtime_dir.join("interventions.log");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let slot = slot.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string());
        // Keep the audit line single-row: collapse newlines in the message.
        let flat = message.replace(['\n', '\r'], " ");
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(
                file,
                "{ts} slot={slot} task={task_id} state={state} bytes={len} outcome={outcome} message={flat}",
                ts = format_iso8601_utc(std::time::SystemTime::now()),
                len = message.len(),
            );
        }
    }
}

impl rhei_tui::InterveneSink for RunInterveneSink {
    fn deliver(&self, task_id: &str, message: &str) -> Result<(), String> {
        // Snapshot the target's identifying metadata + channel under the lock,
        // then release it before writing so a slow agent cannot block others.
        let (tx, log_file, slot, state) = {
            let targets = self.targets.lock().map_err(|_| "intervene registry poisoned".to_string())?;
            match targets.get(task_id) {
                Some(t) => (t.stdin_tx.clone(), t.log_file.clone(), t.slot, t.state.clone()),
                None => {
                    self.audit(task_id, None, "", message, "unreachable");
                    return Err(
                        "agent not interactively reachable (one-shot agent, or task not running)"
                            .to_string(),
                    );
                }
            }
        };

        let mut bytes = message.as_bytes().to_vec();
        bytes.push(b'\n');
        if tx.send(bytes).is_err() {
            self.audit(task_id, Some(slot), &state, message, "stdin-closed");
            return Err("agent stdin is closed".to_string());
        }

        // Mirror into the durable transcript so the message appears inline in
        // the agent log and the live terminal (§FS-rhei-viz §5, AR §10.1).
        let _ = with_agent_log(&log_file, |f| {
            writeln!(f, "[intervene {}] {message}", format_iso8601_utc(std::time::SystemTime::now()))
        });
        self.audit(task_id, Some(slot), &state, message, "delivered");
        Ok(())
    }
}
