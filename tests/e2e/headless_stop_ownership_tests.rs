//! Linux signal authorization for `rhei stop`.
//!
//! Listing may trust contention on the current lock pathname, but stopping must
//! bind the descriptor's exact process identity to the stamped lock owner.
//! §FS-rhei-run-headless.7

#![cfg(target_os = "linux")]

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use super::headless_support::{stderr, stdout, Workspace};
use super::python_command;

struct ChildGuard(Child);

impl ChildGuard {
    fn is_running(&mut self) -> bool {
        self.0.try_wait().expect("inspect child").is_none()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_ready_python(code: &str, args: &[&str]) -> ChildGuard {
    let mut child = Command::new(python_command())
        .arg("-c")
        .arg(code)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn Python fixture");
    let mut ready = String::new();
    BufReader::new(child.stdout.as_mut().expect("captured stdout"))
        .read_line(&mut ready)
        .expect("read fixture readiness");
    assert_eq!(ready.trim(), "READY", "fixture did not become ready");
    ChildGuard(child)
}

fn publish_running_descriptor(ws: &Workspace, id: &str, pid: u32) {
    let workspace = ws.root.to_path_buf();
    let descriptor = serde_json::json!({
        "id": id,
        "pid": pid,
        "status": "running",
        "workspace": workspace,
        "plan": ws.plan(),
        "started_at": "2026-09-01T00:00:00Z",
        "headless": true,
        "parallel": 1,
        "events": ws.root.join("runtime/events.jsonl"),
        "exit_code": null,
    });
    let body = serde_json::to_string_pretty(&descriptor).expect("render descriptor");
    fs::create_dir_all(ws.root.join("runtime")).expect("runtime directory");
    fs::write(ws.root.join("runtime/run.json"), &body).expect("workspace descriptor");
    let registry = ws.home.join("state/rhei/runs");
    fs::create_dir_all(&registry).expect("registry directory");
    fs::write(registry.join(format!("{id}.json")), body).expect("registry descriptor");
}

/// A held current pathname proves that *somebody* is live, not that the pid in
/// matching stale descriptors is its owner. The victim must never receive the
/// stop intended for the fabricated run. §FS-rhei-run-headless.7
#[test]
fn stop_refuses_a_descriptor_pid_that_does_not_own_the_contended_lock() {
    let ws = Workspace::new("stop-unrelated-holder", 30);
    let lock = ws.root.join(".rhei/run.lock");
    fs::create_dir_all(lock.parent().expect("lock directory")).expect("lock directory");
    fs::write(&lock, []).expect("lock file");

    let lock_arg = lock.to_string_lossy().into_owned();
    let mut holder = spawn_ready_python(
        "import fcntl,sys,time\nf=open(sys.argv[1], 'r+')\nfcntl.flock(f, fcntl.LOCK_EX)\nprint('READY', flush=True)\ntime.sleep(60)",
        &[&lock_arg],
    );
    let marker = ws.root.join("victim-signalled");
    let marker_arg = marker.to_string_lossy().into_owned();
    let mut victim = spawn_ready_python(
        "import pathlib,signal,sys,time\np=pathlib.Path(sys.argv[1])\nsignal.signal(signal.SIGINT, lambda *_: p.write_text('SIGINT\\n'))\nprint('READY', flush=True)\nwhile True: time.sleep(1)",
        &[&marker_arg],
    );
    publish_running_descriptor(&ws, "reuse1", victim.0.id());

    let out = ws.rhei(&["stop", "reuse1"]);
    thread::sleep(Duration::from_millis(200));

    assert!(!out.status.success(), "an unowned pid is not a successful stop");
    assert!(!stdout(&out).contains("Asked run"), "no signal was authorized: {}", stdout(&out));
    assert!(
        stderr(&out).contains("does not own its recorded run"),
        "the refusal explains the missing invariant: {}",
        stderr(&out)
    );
    assert!(!marker.exists(), "the unrelated victim received SIGINT");
    assert!(victim.is_running(), "the victim should still be running");
    assert!(holder.is_running(), "the separate lock holder should still be running");
}

/// The same proof accepts the real process after its held inode is renamed and
/// an unlocked replacement takes the pathname. §FS-rhei-run-headless.7
#[test]
fn stop_signals_the_real_owner_of_a_displaced_run_lock() {
    let ws = Workspace::new("stop-displaced-owner", 30);
    let id = ws.launch_headless();
    let lock = ws.root.join(".rhei/run.lock");
    fs::rename(&lock, ws.root.join(".rhei/run.lock.displaced")).expect("rename held lock");
    fs::write(&lock, []).expect("free replacement lock");

    let out = ws.rhei(&["stop", "--wait", &id]);
    let text = stdout(&out);
    assert!(out.status.success(), "the exact owner remains stoppable: {}", stderr(&out));
    assert!(text.contains(&format!("Asked run {id}")), "the signal was delivered: {text}");
    assert!(text.contains("It exited 130"), "the run recorded its interruption: {text}");
}
