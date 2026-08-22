//! Queueing behind another run's lock, and taking that decision back.
//!
//! A foreground run blocks on a contended `.rhei/run.lock` on purpose and says
//! whose run it is waiting for. A wait a command announces has to be one the
//! operator can cancel, which a `flock` the run's own signal handler cannot
//! reach is not.

// §FS-rhei-run.2.6 §FS-rhei-run.3.2

#![cfg(unix)]

use std::fs;
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::headless_support::{stdout, wait_until, Workspace};

/// `kill -<signal> <pid>`, through the tool rather than a libc binding: what is
/// under test is the run's response, not the delivery.
fn signal(pid: u32, signal: &str) {
    let status = std::process::Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .status()
        .expect("kill runs");
    assert!(status.success(), "kill -{signal} {pid} failed");
}

/// The operator asks for a run, sees it queue behind a live one, changes their
/// mind. Before this, `Ctrl+C` did nothing at all: the process was parked inside
/// a blocking `flock`, so the handler's flag was never read and the wait ran to
/// whenever the other run happened to finish.
// §FS-rhei-run.2.6 §FS-rhei-run.3.2
#[test]
fn a_run_queued_on_a_contended_lock_can_be_interrupted() {
    let ws = Workspace::new("lock-wait", 30);
    let holder = ws.launch_headless();

    let console = ws.root.join("queued.out");
    let mut queued = ws
        .rhei_command(
            &ws.run_args(&["--no-tui", "--no-dashboard"])
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
        .stdin(Stdio::null())
        .stdout(fs::File::create(&console).expect("console"))
        .stderr(Stdio::null())
        .spawn()
        .expect("the second run starts");

    // It has to actually be waiting before the signal means anything.
    wait_until("the second run to announce its wait", Duration::from_secs(30), || {
        fs::read_to_string(&console).is_ok_and(|text| text.contains("Waiting for run"))
    });
    let announced = fs::read_to_string(&console).expect("console");
    assert!(announced.contains(&holder), "the wait names the run it is queued behind: {announced}");

    let started = Instant::now();
    signal(queued.id(), "INT");
    wait_until("the queued run to give up its wait", Duration::from_secs(15), || {
        matches!(queued.try_wait(), Ok(Some(_)))
    });
    let cancelled = started.elapsed();
    let status = queued.wait().expect("the queued run exits");

    assert!(cancelled < Duration::from_secs(10), "it took {cancelled:?} to notice the signal");
    assert!(!status.success(), "a run that never started is not a run that succeeded");

    // The queueing behaviour is unchanged and the holder is untouched: the
    // operator cancelled their own wait, not somebody else's run.
    let listed = stdout(&ws.rhei(&["runs"]));
    assert!(listed.contains(&holder), "the run holding the lock is still live: {listed}");
    ws.stop_quietly();
}
