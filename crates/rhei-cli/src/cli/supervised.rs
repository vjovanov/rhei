// Supervised subprocess groups.
//
// Every subprocess `rhei run` starts is a process group it owns, with exactly
// one early-termination path and three reasons to take it: the invocation's
// deadline, an operator interrupt, or the supervisor's death. Timeout and
// shutdown are two triggers of the same routine, applied to the group.
// §FS-rhei-run.3.2 §DA-supervised-process-groups

/// Grace between `SIGTERM` and `SIGKILL` when terminating a group — the same
/// 10 s whether a deadline or an interruption fired it. §FS-rhei-run.3.2
#[cfg(not(test))]
const SUPERVISED_TERMINATE_GRACE: Duration = Duration::from_secs(10);
#[cfg(test)]
const SUPERVISED_TERMINATE_GRACE: Duration = Duration::from_millis(50);

/// Poll interval of the supervised wait loop. Waiting is a poll rather than a
/// blocking `wait()` because the loop must also see the stop token.
const SUPERVISED_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Poll interval while waiting out the termination grace.
const SUPERVISED_GRACE_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Longest a scheduler sleep may run before it re-reads the stop token.
const SUPERVISED_SLEEP_SLICE: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Stop token
// ---------------------------------------------------------------------------

/// How many interruptions the run has been asked for. `0` is "running";
/// `>= 2` is the operator asking twice, which skips the termination grace.
/// §FS-rhei-run.3.2
static INTERRUPT_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The first signal that interrupted the run, `0` until one arrives. First one
/// wins, so the exit code names the signal the operator actually sent.
static INTERRUPT_SIGNAL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Raise the stop token without naming a signal. Used by the shutdown guard,
/// which stops in-flight work on an early error or a panic unwind — neither of
/// which should change the process's exit code. §FS-rhei-run.3.2
fn request_interrupt() {
    bump_interrupt_level();
}

/// Raise the stop token by one. Saturating, so a wedged operator leaning on
/// Ctrl+C cannot wrap the counter back to "not interrupted".
fn bump_interrupt_level() {
    let _ = INTERRUPT_LEVEL.fetch_update(
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
        |level| Some(level.saturating_add(1)),
    );
}

/// How many interruptions have been requested so far.
fn interrupt_level() -> u8 {
    INTERRUPT_LEVEL.load(std::sync::atomic::Ordering::SeqCst)
}

/// Whether the run is shutting down: schedule nothing new, end every wait.
/// §FS-rhei-run.3.2
fn interrupt_requested() -> bool {
    interrupt_level() > 0
}

/// Whether the operator asked twice, which means "kill the group now" rather
/// than "ask it to stop and wait 10 s". §FS-rhei-run.3.2
fn interrupt_skip_grace() -> bool {
    interrupt_level() >= 2
}

/// The signal that interrupted the run, if one did.
fn interrupt_signal_number() -> Option<i32> {
    match INTERRUPT_SIGNAL.load(std::sync::atomic::Ordering::SeqCst) {
        0 => None,
        signum => Some(signum),
    }
}

/// The process exit status for an interrupted run: `128 + signal`, the same
/// value a shell reports for a process the signal killed. §FS-rhei-run.3.2
fn interrupt_exit_code() -> Option<i32> {
    interrupt_signal_number().map(|signum| 128 + signum)
}

/// Restore the token to "running". Tests only — a live run never un-interrupts.
#[cfg(test)]
fn reset_interrupt_state() {
    INTERRUPT_LEVEL.store(0, std::sync::atomic::Ordering::SeqCst);
    INTERRUPT_SIGNAL.store(0, std::sync::atomic::Ordering::SeqCst);
}

/// The one handler for every interrupting signal.
///
/// Async-signal-safe by construction: it touches two lock-free atomics and
/// does nothing else — no allocation, no locking, no I/O. Every decision that
/// follows from an interrupt is taken by the loops that poll the token.
/// §FS-rhei-run.3.2
#[cfg(unix)]
extern "C" fn interrupt_signal_handler(signum: std::ffi::c_int) {
    let _ = INTERRUPT_SIGNAL.compare_exchange(
        0,
        signum,
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
    );
    bump_interrupt_level();
}

/// Install the interruption handler for `SIGINT`, `SIGTERM`, and `SIGHUP`.
///
/// `SA_RESTART` so an interrupt does not turn every in-progress read in the
/// process into an `EINTR` failure. `SIGPIPE` is deliberately untouched: this
/// CLI writes to pipes it owns and needs `EPIPE` back as a value
/// (see [`install_quiet_broken_pipe_exit`]). §FS-rhei-run.3.2
#[cfg(unix)]
fn install_interrupt_handlers() {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet};

    let action = SigAction::new(
        SigHandler::Handler(interrupt_signal_handler),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    for sig in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the handler is async-signal-safe (two atomics, no more).
        unsafe {
            let _ = signal::sigaction(sig, &action);
        }
    }
}

/// No interruption handling off Unix; the run keeps the platform default.
#[cfg(not(unix))]
fn install_interrupt_handlers() {}

/// Sleep up to `total`, returning early once the run is interrupted.
///
/// The scheduler's idle waits are minutes long (a poll deadline, a human
/// gate); sleeping them out whole would keep a run alive long after the
/// operator asked it to stop. §FS-rhei-run.3.2
fn interruptible_sleep(total: Duration) {
    let deadline = Instant::now() + total;
    loop {
        if interrupt_requested() {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        std::thread::sleep(SUPERVISED_SLEEP_SLICE.min(deadline - now));
    }
}

// ---------------------------------------------------------------------------
// Live group registry
// ---------------------------------------------------------------------------

/// Every supervised subprocess that has not been reaped: its process-group id
/// against the `<task>@<state>` label of the invocation that owns it.
///
/// This is what makes the shutdown paths a handler cannot reach — an early `?`
/// return, a panic unwind — able to tear down work owned by other threads, and
/// what lets the shutdown notice name what it is stopping. §FS-rhei-run.3.2
#[cfg(unix)]
static LIVE_GROUPS: Mutex<BTreeMap<i32, String>> = Mutex::new(BTreeMap::new());

#[cfg(unix)]
fn register_live_group(pgid: i32, label: &str) {
    if let Ok(mut live) = LIVE_GROUPS.lock() {
        live.insert(pgid, label.to_string());
    }
}

#[cfg(unix)]
fn unregister_live_group(pgid: i32) {
    if let Ok(mut live) = LIVE_GROUPS.lock() {
        live.remove(&pgid);
    }
}

/// Snapshot of the live groups. A poisoned lock degrades to "none known"
/// rather than panicking a shutdown path.
#[cfg(unix)]
fn live_group_ids() -> Vec<i32> {
    LIVE_GROUPS.lock().map(|live| live.keys().copied().collect()).unwrap_or_default()
}

#[cfg(not(unix))]
fn live_group_ids() -> Vec<i32> {
    Vec::new()
}

/// The `<task>@<state>` labels of the invocations still running.
#[cfg(unix)]
fn live_invocation_labels() -> Vec<String> {
    LIVE_GROUPS.lock().map(|live| live.values().cloned().collect()).unwrap_or_default()
}

#[cfg(not(unix))]
fn live_invocation_labels() -> Vec<String> {
    Vec::new()
}

/// How many supervised groups are still running.
fn live_group_count() -> usize {
    live_group_ids().len()
}

/// Set once the shutdown notice has been written, so several workers noticing
/// the same interrupt produce one line rather than one line each.
static INTERRUPT_ANNOUNCED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Tell the operator what the interruption is doing, and that a second signal
/// skips the grace.
///
/// Written to stderr rather than through the event sink: under the TUI the
/// render loop has already exited by the time a Ctrl+C reaches the engine, so
/// events emitted after it are dropped — and the terminal it restored is
/// exactly where this line belongs. The per-invocation `interrupted` outcome
/// still reaches the journal, dashboard, and report the ordinary way.
/// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
fn announce_interruption_once() {
    if INTERRUPT_ANNOUNCED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let labels = live_invocation_labels();
    if labels.is_empty() {
        eprintln!("\nInterrupted — no subprocess in flight; stopping the run.");
    } else {
        eprintln!(
            "\nInterrupted — terminating {} invocation(s) ({}); \
             press Ctrl+C again to kill immediately.",
            labels.len(),
            labels.join(", ")
        );
    }
}

/// Terminate every registered group with the shared sequence: `SIGTERM`, the
/// grace, then `SIGKILL` on whatever is left. Signals only — each group's
/// owning thread reaps its own child. §FS-rhei-run.3.2
#[cfg(unix)]
fn terminate_live_groups() {
    let groups = live_group_ids();
    if groups.is_empty() {
        return;
    }
    if !interrupt_skip_grace() {
        for pgid in &groups {
            let _ = signal::killpg(Pid::from_raw(*pgid), Signal::SIGTERM);
        }
        let deadline = Instant::now() + SUPERVISED_TERMINATE_GRACE;
        while Instant::now() < deadline {
            if live_group_ids().is_empty() {
                return;
            }
            if interrupt_skip_grace() {
                break;
            }
            std::thread::sleep(SUPERVISED_GRACE_POLL_INTERVAL);
        }
    }
    for pgid in live_group_ids() {
        let _ = signal::killpg(Pid::from_raw(pgid), Signal::SIGKILL);
    }
}

#[cfg(not(unix))]
fn terminate_live_groups() {}

/// Declared alongside [`RunReportGuard`] so it runs on **every** way out of an
/// execution mode — an early `?`, a panic unwind, or a normal end. Without it,
/// an error return after workers were spawned leaves exactly the orphans this
/// design exists to prevent. §FS-rhei-run.3.2
struct RunSubprocessGuard;

impl Drop for RunSubprocessGuard {
    fn drop(&mut self) {
        if live_group_count() == 0 {
            return;
        }
        // Stop the waits first, or a worker would re-enter its own termination
        // sequence against a group this one is already tearing down.
        request_interrupt();
        terminate_live_groups();
    }
}

// ---------------------------------------------------------------------------
// Supervised subprocess
// ---------------------------------------------------------------------------

/// Why a supervised subprocess stopped. §FS-rhei-run.3.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndCause {
    /// It exited on its own.
    Exited,
    /// Its deadline fired and the engine terminated its group.
    TimedOut,
    /// The run was interrupted and the engine terminated its group.
    Interrupted,
}

/// A finished supervised invocation: the status reaped from the direct child,
/// and why it stopped.
#[derive(Debug)]
struct Ended {
    status: std::process::ExitStatus,
    cause: EndCause,
}

/// A subprocess and the process group it leads.
struct Supervised {
    child: std::process::Child,
    /// The group id, which equals the leader's pid. Unix only: Windows keeps
    /// the single-child `kill()` it always had.
    #[cfg(unix)]
    pgid: i32,
    /// Set once the child has been reaped, so `Drop` knows whether it still
    /// owns a live group.
    reaped: bool,
}

impl Supervised {
    /// Spawn `cmd` as the leader of its own process group and register the
    /// group with the run's shutdown path under `label` (`<task>@<state>`).
    /// §FS-rhei-run.3.2
    fn spawn(cmd: &mut std::process::Command, label: &str) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            // The descendants inherit it, which is the whole point: an agent's
            // MCP servers and shell tools are terminated with it.
            cmd.process_group(0);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt as _;
            // Captured before the fork so the child can tell "my parent is
            // still the process that spawned me" from "it already died and I
            // was reparented" — `getppid() == 1` is not that test, because a
            // subreaper adopts orphans instead of init.
            let supervisor = std::process::id();
            // SAFETY: this runs between fork and exec in the child. It makes
            // two syscalls, allocates nothing, and takes no lock.
            unsafe {
                cmd.pre_exec(move || {
                    // Backstop for the deaths no handler can catch — SIGKILL,
                    // OOM — where the supervisor runs no code at all. The
                    // per-*thread* semantics are harmless here: the thread that
                    // spawns a subprocess is the one that waits on it, so it
                    // outlives the child on every path short of the whole
                    // process dying. §FS-rhei-run.3.2
                    let _ = nix::sys::prctl::set_pdeathsig(Some(Signal::SIGTERM));
                    // Arming it happens after the fork, so a supervisor that
                    // died in between would never deliver it. Close the window.
                    if nix::unistd::getppid().as_raw() as u32 != supervisor {
                        return Err(std::io::Error::from_raw_os_error(
                            nix::errno::Errno::ESRCH as i32,
                        ));
                    }
                    Ok(())
                });
            }
        }

        let child = cmd.spawn()?;
        #[cfg(unix)]
        let pgid = child.id() as i32;
        #[cfg(unix)]
        register_live_group(pgid, label);
        #[cfg(not(unix))]
        let _ = label;
        Ok(Self {
            child,
            #[cfg(unix)]
            pgid,
            reaped: false,
        })
    }

    /// Wait for the subprocess, its deadline, or the run's interruption —
    /// whichever comes first. The last two run the identical termination
    /// sequence against the group and differ only in the cause reported.
    /// §FS-rhei-run.3.2
    fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<Ended> {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.finish(status);
                return Ok(Ended { status, cause: EndCause::Exited });
            }
            let timed_out = timeout.is_some_and(|limit| start.elapsed() > limit);
            if timed_out || interrupt_requested() {
                let cause = if timed_out {
                    EndCause::TimedOut
                } else {
                    // The first waiter to notice names every invocation the
                    // shutdown is about to end. §FS-rhei-run.3.2
                    announce_interruption_once();
                    EndCause::Interrupted
                };
                let status = self.terminate_and_reap()?;
                return Ok(Ended { status, cause });
            }
            std::thread::sleep(SUPERVISED_POLL_INTERVAL);
        }
    }

    /// `SIGTERM` the group, wait out the grace, then `SIGKILL` it — unless the
    /// operator already asked twice, which skips straight to the kill.
    /// §FS-rhei-run.3.2
    fn terminate_and_reap(&mut self) -> std::io::Result<std::process::ExitStatus> {
        if !interrupt_skip_grace() {
            self.terminate_group();
            let deadline = Instant::now() + SUPERVISED_TERMINATE_GRACE;
            while Instant::now() < deadline {
                if let Some(status) = self.child.try_wait()? {
                    self.finish(status);
                    return Ok(status);
                }
                // A second interrupt mid-grace is the operator saying "now".
                if interrupt_skip_grace() {
                    break;
                }
                std::thread::sleep(SUPERVISED_GRACE_POLL_INTERVAL);
            }
        }
        self.kill_group();
        let status = self.child.wait()?;
        self.finish(status);
        Ok(status)
    }

    /// Record that the child has been reaped and drop the group's registration.
    fn finish(&mut self, _status: std::process::ExitStatus) {
        self.reaped = true;
        #[cfg(unix)]
        unregister_live_group(self.pgid);
    }

    /// Ask the whole group to stop.
    #[cfg(unix)]
    fn terminate_group(&mut self) {
        let _ = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGTERM);
    }

    /// Windows has no process group to signal here, so the direct child is
    /// killed exactly as it was before this change. §FS-rhei-run.3.2
    #[cfg(not(unix))]
    fn terminate_group(&mut self) {
        let _ = self.child.kill();
    }

    /// End the whole group now.
    #[cfg(unix)]
    fn kill_group(&mut self) {
        let _ = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGKILL);
    }

    #[cfg(not(unix))]
    fn kill_group(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for Supervised {
    fn drop(&mut self) {
        #[cfg(unix)]
        unregister_live_group(self.pgid);
        if !self.reaped {
            // Left by an error or a panic before the wait finished: the group
            // must not outlive the invocation that owns it. §FS-rhei-run.3.2
            self.kill_group();
            let _ = self.child.wait();
        }
    }
}
