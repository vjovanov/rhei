// AR §7: the host side of the one mutation boundary. `RunInterveneSink` keeps a
// per-slot registry of open agent-stdin writers, delivers `/intervene` messages
// to selected stdin handles and nothing else, and records every delivery (and
// every failure reason) to a durable audit trail at `runtime/interventions.log`,
// mirroring successful messages into the task's agent transcript.
//
// There is no path from here to plan-state mutation: a delivery resolves a task,
// sends bytes to that task's child stdin, logs, and returns.

use std::sync::mpsc::{channel, Sender};

/// One registered running invocation: a channel to its stdin writer thread, the
/// shared handle to its durable agent log (so a mirror line cannot interleave
/// with the agent's own output), and identifying metadata for the audit trail.
struct InterveneTarget {
    task_id: String,
    stdin_tx: Sender<Vec<u8>>,
    log_file: Arc<Mutex<fs::File>>,
    slot: rhei_tui::Slot,
    state: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct InterveneKey {
    task_id: String,
    slot: rhei_tui::Slot,
}

pub(crate) struct RunInterveneSink {
    runtime_dir: PathBuf,
    targets: Mutex<HashMap<InterveneKey, InterveneTarget>>,
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
            let key = InterveneKey { task_id: task_id.to_string(), slot };
            targets.insert(
                key,
                InterveneTarget {
                    task_id: task_id.to_string(),
                    stdin_tx: tx,
                    log_file,
                    slot,
                    state: state.to_string(),
                },
            );
        }
    }

    /// Drop a task's registration once its agent exits. Dropping the stored
    /// sender ends the writer thread, which closes the child's stdin.
    pub(crate) fn unregister(&self, task_id: &str, slot: rhei_tui::Slot) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.remove(&InterveneKey { task_id: task_id.to_string(), slot });
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
    fn deliver(
        &self,
        task_id: Option<&str>,
        slot: Option<rhei_tui::Slot>,
        message: &str,
    ) -> Result<(), String> {
        // Snapshot the target's identifying metadata + channel under the lock,
        // then release it before writing so a slow agent cannot block others.
        let targets = {
            let targets = self.targets.lock().map_err(|_| "intervene registry poisoned".to_string())?;
            let exact = slot.and_then(|slot| {
                if let Some(task_id) = task_id {
                    targets.get(&InterveneKey { task_id: task_id.to_string(), slot })
                } else {
                    targets.values().find(|target| target.slot == slot)
                }
            });

            let mut selected = Vec::new();
            if let Some(target) = exact {
                selected.push(target);
            } else if let Some(task_id) = task_id {
                selected.extend(targets.values().filter(|target| target.task_id == task_id));
            }

            selected
                .into_iter()
                .map(|target| {
                    (
                        target.task_id.clone(),
                        target.stdin_tx.clone(),
                        target.log_file.clone(),
                        target.slot,
                        target.state.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };

        if targets.is_empty() {
            self.audit(task_id.unwrap_or("-"), slot, "", message, "unreachable");
            return Err(
                "agent not interactively reachable (one-shot agent, or task not running)"
                    .to_string(),
            );
        }
        if slot.is_none() && task_id.is_some() && targets.len() > 1 {
            self.audit(task_id.unwrap_or("-"), None, "", message, "ambiguous");
            return Err("multiple agents are running for this task; retry with slot".to_string());
        }

        let mut delivered = 0usize;
        let mut failed = 0usize;
        for (task_id, tx, log_file, slot, state) in targets {
            let mut bytes = message.as_bytes().to_vec();
            bytes.push(b'\n');
            if tx.send(bytes).is_err() {
                self.audit(&task_id, Some(slot), &state, message, "stdin-closed");
                failed += 1;
                continue;
            }

            // Mirror into the durable transcript so the message appears inline
            // in the agent log and the live terminal (§FS-rhei-viz §5, AR §10.1).
            let _ = with_agent_log(&log_file, |f| {
                writeln!(
                    f,
                    "[intervene {}] {message}",
                    format_iso8601_utc(std::time::SystemTime::now())
                )
            });
            self.audit(&task_id, Some(slot), &state, message, "delivered");
            delivered += 1;
        }

        match (delivered, failed) {
            (0, _) => Err("agent stdin is closed".to_string()),
            (_, 0) => Ok(()),
            _ => Err(format!(
                "delivered to {delivered} agent(s), but {failed} active agent stdin stream(s) were closed"
            )),
        }
    }
}

#[cfg(test)]
mod intervene_tests {
    use super::*;
    use std::time::Duration;

    fn temp_log(name: &str) -> Arc<Mutex<fs::File>> {
        let path = std::env::temp_dir().join(format!(
            "rhei-intervene-{}-{name}.log",
            std::process::id()
        ));
        Arc::new(Mutex::new(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("open temp intervention log"),
        ))
    }

    #[test]
    fn unregister_removes_only_the_finished_slot_for_a_task() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (first_tx, first_rx) = channel::<Vec<u8>>();
        let (second_tx, second_rx) = channel::<Vec<u8>>();

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(
                InterveneKey { task_id: "1".to_string(), slot: 0 },
                InterveneTarget {
                    task_id: "1".to_string(),
                    stdin_tx: first_tx,
                    log_file: temp_log("first"),
                    slot: 0,
                    state: "review".to_string(),
                },
            );
            targets.insert(
                InterveneKey { task_id: "1".to_string(), slot: 1 },
                InterveneTarget {
                    task_id: "1".to_string(),
                    stdin_tx: second_tx,
                    log_file: temp_log("second"),
                    slot: 1,
                    state: "review".to_string(),
                },
            );
        }

        sink.unregister("1", 0);
        rhei_tui::InterveneSink::deliver(&sink, Some("1"), None, "still running")
            .expect("deliver to remaining slot");

        assert!(matches!(
            first_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty | std::sync::mpsc::TryRecvError::Disconnected)
        ));
        assert_eq!(
            second_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("message reaches sibling"),
            b"still running\n".to_vec()
        );
    }

    #[test]
    fn task_only_delivery_rejects_ambiguous_fanout() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (first_tx, first_rx) = channel::<Vec<u8>>();
        let (second_tx, second_rx) = channel::<Vec<u8>>();

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(
                InterveneKey { task_id: "1".to_string(), slot: 0 },
                InterveneTarget {
                    task_id: "1".to_string(),
                    stdin_tx: first_tx,
                    log_file: temp_log("ambiguous-first"),
                    slot: 0,
                    state: "review".to_string(),
                },
            );
            targets.insert(
                InterveneKey { task_id: "1".to_string(), slot: 1 },
                InterveneTarget {
                    task_id: "1".to_string(),
                    stdin_tx: second_tx,
                    log_file: temp_log("ambiguous-second"),
                    slot: 1,
                    state: "review".to_string(),
                },
            );
        }

        let err = rhei_tui::InterveneSink::deliver(&sink, Some("1"), None, "ambiguous")
            .expect_err("task-only delivery must not broadcast to fanout siblings");

        assert!(err.contains("retry with slot"));
        assert!(first_rx.try_recv().is_err());
        assert!(second_rx.try_recv().is_err());
    }
}
