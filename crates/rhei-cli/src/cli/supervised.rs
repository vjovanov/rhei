// Supervised subprocess groups.
//
// Every subprocess `rhei run` starts to do a ticket's work — agents, programs,
// and the snapshot redactor — is a process group it owns, with exactly one
// early-termination path and three reasons to take it: the invocation's
// deadline, an operator interrupt, or the supervisor's death. Timeout and
// shutdown are two triggers of the same routine, applied to the group.
//
// Three subprocesses are deliberately outside this, each for its own reason:
// a subprocess a *callback* starts is that callback's own child and the
// callback contract governs it; the `git` queries of the post-transition
// consistency check are short synchronous bookkeeping that ends before the
// call returns; and the editor the dashboard launches is detached on purpose,
// because it belongs to the operator and outliving the run is the point.

// §FS-rhei-run.3.2 §DA-supervised-process-groups

/// Grace between `SIGTERM` and `SIGKILL` when terminating a group — the same
/// 10 s whether a deadline or an interruption fired it. §FS-rhei-run.3.2
#[cfg(not(test))]
const SUPERVISED_TERMINATE_GRACE: Duration = Duration::from_secs(10);
#[cfg(test)]
const SUPERVISED_TERMINATE_GRACE: Duration = Duration::from_millis(50);

/// First poll interval of the supervised wait loop. Waiting is a poll rather
/// than a blocking `wait()` because the loop must also see the stop token, and
/// the price of that is latency the child never had: whatever the loop is
/// asleep for when the child exits.
const SUPERVISED_POLL_MIN: Duration = Duration::from_millis(10);

/// Ceiling the wait loop's poll interval ramps up to.
///
/// A single interval has to be either wasteful for an agent that runs for
/// minutes or slow for a redactor that finishes in tens of milliseconds. The
/// ramp is neither: short invocations are noticed almost at once, and a long
/// one settles into a cheap idle poll within a couple of seconds.
const SUPERVISED_POLL_MAX: Duration = Duration::from_millis(200);

/// The next poll interval after an idle pass: double, then stop at the cap.
fn next_poll_interval(current: Duration) -> Duration {
    (current * 2).min(SUPERVISED_POLL_MAX)
}

/// Poll interval while waiting out the termination grace.
const SUPERVISED_GRACE_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Longest a scheduler sleep may run before it re-reads the stop token.
const SUPERVISED_SLEEP_SLICE: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Stop token
// ---------------------------------------------------------------------------

/// The run's stop token: raised by the signal handler, polled by every loop
/// that would otherwise keep working.
///
/// A value rather than a set of loose statics so the wait routine can be
/// exercised against a token a test owns, instead of the process-wide one.
///
/// Two things are counted separately because they are two different facts.
/// `stopping` is "end every wait and start nothing more", which an operator's
/// signal and an error unwind both mean. `signals` is how many times the
/// *operator* asked, and only that: escalating to an immediate `SIGKILL` is
/// something a person asks for twice, never something a failed `?` on another
/// thread can arrange on their behalf.
// §FS-rhei-run.3.2: one interruption contract for the whole run.
struct StopToken {
    /// Whether the run is shutting down at all, whoever asked.
    stopping: std::sync::atomic::AtomicBool,
    /// How many times the operator has signalled. `>= 2` is the operator
    /// asking twice, which skips the termination grace.
    signals: std::sync::atomic::AtomicU8,
    /// The first signal that raised this token, `0` until one does. First one
    /// wins, so the exit code names the signal the operator actually sent.
    signal: std::sync::atomic::AtomicI32,
    /// Whether the shutdown notice has been written, so several workers
    /// noticing the same interrupt produce one line rather than one each.
    announced: std::sync::atomic::AtomicBool,
}

impl StopToken {
    const fn new() -> Self {
        Self {
            stopping: std::sync::atomic::AtomicBool::new(false),
            signals: std::sync::atomic::AtomicU8::new(0),
            signal: std::sync::atomic::AtomicI32::new(0),
            announced: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Raise the token without naming a signal. Used by the shutdown guard,
    /// which stops in-flight work on an early error or a panic unwind —
    /// neither of which should change the process's exit code, and neither of
    /// which is the operator asking for the grace to be skipped.
    fn request(&self) {
        self.stopping.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Lower a teardown-raised flag once the run that raised it has finished
    /// tearing its own groups down.
    ///
    /// The flag is process-wide but the reason for it was one run's, and a
    /// process can drive more than one (the in-process tests do). Left raised,
    /// it would make every later run in the process skip its work on the first
    /// pass and report success without doing any. A *signal* is never lowered:
    /// that one stopped the process, not just the run.
    // §FS-rhei-run.3.2
    fn release_teardown(&self) {
        if self.signal_number().is_some() {
            return;
        }
        self.stopping.store(false, std::sync::atomic::Ordering::SeqCst);
        self.announced.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Raise the token and record the signal that did it. Called from the
    /// signal handler, so everything it touches must be lock-free.
    ///
    /// Unix-only, because naming a signal is: off Unix nothing installs a
    /// handler and the run is stopped through [`StopToken::request`] alone.
    #[cfg(unix)]
    fn raise(&self, signum: i32) {
        let _ = self.signal.compare_exchange(
            0,
            signum,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        );
        // Saturating, so an operator leaning on Ctrl+C cannot wrap the counter
        // back round to "not interrupted".
        let _ = self.signals.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |count| Some(count.saturating_add(1)),
        );
        self.request();
    }

    /// Whether the run is shutting down: schedule nothing new, end every wait.
    fn is_set(&self) -> bool {
        self.stopping.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times the operator has signalled.
    fn signals_received(&self) -> u8 {
        self.signals.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether the operator asked twice, which means "kill the group now"
    /// rather than "ask it to stop and wait out the grace". §FS-rhei-run.3.2
    fn skip_grace(&self) -> bool {
        self.signals_received() >= 2
    }

    fn signal_number(&self) -> Option<i32> {
        match self.signal.load(std::sync::atomic::Ordering::SeqCst) {
            0 => None,
            signum => Some(signum),
        }
    }

    /// `128 + signal`, the status a shell reports for a process the signal
    /// killed. `None` when the token was raised without one. §FS-rhei-run.3.2
    fn exit_code(&self) -> Option<i32> {
        self.signal_number().map(|signum| 128 + signum)
    }

    /// The shutdown notice — what is being terminated, and that a second signal
    /// skips the grace — handed to **one** caller and `None` to every other, so
    /// several workers noticing the same interrupt produce one line rather than
    /// one each.
    ///
    /// Text, never I/O. Where the line belongs depends on what the operator is
    /// looking at — a live TUI journal pane, a terminal the TUI has already
    /// restored, a redirected stdout — and that is the frontend's question.
    /// Writing it here sent an external `SIGTERM` arriving mid-render straight
    /// into the alternate screen.
    // §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
    fn take_announcement(&self) -> Option<String> {
        if self.announced.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let labels = live_invocation_labels();
        Some(if labels.is_empty() {
            "\nInterrupted — no subprocess in flight; stopping the run.".to_string()
        } else {
            format!(
                "\nInterrupted — terminating {} invocation(s) ({}); \
                 press Ctrl+C again to kill immediately.",
                labels.len(),
                labels.join(", ")
            )
        })
    }
}

/// The process's one stop token. A `rhei run` process drives exactly one run,
/// so the signal that interrupts it interrupts all of it. §FS-rhei-run.3.2
static INTERRUPT: StopToken = StopToken::new();

/// Whether the run is shutting down.
fn interrupt_requested() -> bool {
    INTERRUPT.is_set()
}

/// Whether an operator's signal stopped this run, as opposed to the shutdown
/// guard stopping in-flight work on the way out of a failure.
///
/// Both raise the same token, because both mean "start nothing more and end
/// every wait". They mean opposite things to a *reader*, though: one says the
/// operator stopped a healthy run, the other that the run is unwinding from an
/// error it is about to report. Only the first belongs in the run's result.
// §FS-rhei-run.3.2 §FS-rhei-run-report.3.1
fn interrupted_by_signal() -> bool {
    INTERRUPT.signal_number().is_some()
}

/// The process exit status for an interrupted run. §FS-rhei-run.3.2
fn interrupt_exit_code() -> Option<i32> {
    INTERRUPT.exit_code()
}

/// The run's shutdown notice, for the first caller to ask. §FS-rhei-run.3.2
fn take_interruption_announcement() -> Option<String> {
    INTERRUPT.take_announcement()
}

/// A `notify` for [`Supervised::wait`] that puts the shutdown notice on the
/// run's event stream, where the frontend in use decides where it is legible.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
fn notify_through_sink(sink: &Arc<dyn rhei_tui::EventSink>) -> impl Fn(String) + '_ {
    move |text| {
        sink.emit(rhei_tui::RunEvent::Message { level: rhei_tui::MessageLevel::Warn, text })
    }
}

/// The one handler for every interrupting signal.
///
/// Async-signal-safe by construction: it touches two lock-free atomics and
/// does nothing else — no allocation, no locking, no I/O. Every decision that
/// follows from an interrupt is taken by the loops that poll the token.
// §FS-rhei-run.3.2: SIGINT, SIGTERM, and SIGHUP interrupt the run.
#[cfg(all(unix, not(test)))]
extern "C" fn interrupt_signal_handler(signum: std::ffi::c_int) {
    INTERRUPT.raise(signum);
}

/// Install the interruption handler for `SIGINT`, `SIGTERM`, and `SIGHUP`.
///
/// `SA_RESTART` so an interrupt does not turn every in-progress read in the
/// process into an `EINTR` failure. `SIGPIPE` is deliberately untouched: this
/// CLI writes to pipes it owns and needs `EPIPE` back as a value
/// (see [`install_quiet_broken_pipe_exit`]).
// §FS-rhei-run.3.2: one handler, installed for every `rhei run`.
#[cfg(all(unix, not(test)))]
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

/// Nothing to install here.
///
/// Off Unix the run keeps the platform default. Under `cfg(test)` the reason
/// is different: signal dispositions are process-wide, and the in-process
/// tests that call `run_command` would leave the whole test binary swallowing
/// its own Ctrl+C into a token nothing reads. The handler is exercised where
/// it matters — the interruption e2e tests signal the real binary.
// §FS-rhei-run.3.2
#[cfg(any(not(unix), test))]
fn install_interrupt_handlers() {}

/// Sleep up to `total`, returning early once the run is interrupted.
///
/// The scheduler's idle waits are minutes long (a poll deadline, a human
/// gate); sleeping them out whole would keep a run alive long after the
/// operator asked it to stop.
// §FS-rhei-run.3.2: an interrupted run schedules nothing further.
fn interruptible_sleep(total: Duration) {
    let deadline = Instant::now() + total;
    loop {
        if INTERRUPT.is_set() {
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

/// One live supervised subprocess: the run that owns it and the
/// `<task>@<state>` label of the invocation it belongs to.
#[cfg(unix)]
struct LiveGroup {
    owner: u64,
    label: String,
    /// Whether this group has already been asked to stop. Both the shutdown
    /// guard and the group's own waiter can reach the same group, and the
    /// second one to arrive has nothing to add by re-sending `SIGTERM`.
    asked_to_stop: bool,
}

/// Every supervised subprocess that has not been reaped, keyed by process-group
/// id.
///
/// This is what makes the shutdown paths a handler cannot reach — an early `?`
/// return, a panic unwind — able to tear down work owned by other threads, and
/// what lets the shutdown notice name what it is stopping.
// §FS-rhei-run.3.2: the supervisor's death ends its subprocesses.
#[cfg(unix)]
static LIVE_GROUPS: Mutex<BTreeMap<i32, LiveGroup>> = Mutex::new(BTreeMap::new());

/// Source of run ids. A `rhei run` process drives one run, so this only ever
/// hands out more than one id under the in-process tests.
static NEXT_RUN_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

thread_local! {
    /// The run whose subprocesses this thread spawns. `0` means "no run owns
    /// them" — a direct `spawn_and_wait_agent` from a test, say — and no
    /// shutdown guard will claim them. Worker threads inherit it explicitly
    /// from the thread that started them ([`inherit_run_owner`]).
    static RUN_OWNER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The run this thread's subprocesses belong to.
fn current_run_owner() -> u64 {
    RUN_OWNER.with(std::cell::Cell::get)
}

fn set_run_owner(owner: u64) {
    RUN_OWNER.with(|cell| cell.set(owner));
}

/// Adopt the calling run on a worker thread, so the subprocesses it spawns are
/// torn down by that run's guard and by no other. §FS-rhei-run.3.2
fn inherit_run_owner(owner: u64) {
    set_run_owner(owner);
}

#[cfg(unix)]
fn register_live_group(pgid: i32, label: &str) {
    if let Ok(mut live) = LIVE_GROUPS.lock() {
        live.insert(
            pgid,
            LiveGroup {
                owner: current_run_owner(),
                label: label.to_string(),
                asked_to_stop: false,
            },
        );
    }
}

/// Claim the right to send this group its `SIGTERM`, returning whether the
/// caller is the first to ask. A group nobody registered — one already reaped —
/// is nobody's to ask, so the claim fails.
#[cfg(unix)]
fn claim_group_termination(pgid: i32) -> bool {
    LIVE_GROUPS
        .lock()
        .map(|mut live| {
            live.get_mut(&pgid)
                .is_some_and(|group| !std::mem::replace(&mut group.asked_to_stop, true))
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn unregister_live_group(pgid: i32) {
    if let Ok(mut live) = LIVE_GROUPS.lock() {
        live.remove(&pgid);
    }
}

/// The live process-group ids, filtered to one run's own when `owner` names
/// one and taken wholesale when it is `None`.
///
/// A poisoned lock degrades to "none known" rather than panicking a shutdown
/// path. It cannot deadlock: nothing that holds this lock prints, allocates a
/// diagnostic, or can otherwise re-enter a shutdown path while holding it.
#[cfg(unix)]
fn live_group_ids(owner: Option<u64>) -> Vec<i32> {
    LIVE_GROUPS
        .lock()
        .map(|live| {
            live.iter()
                .filter(|(_, group)| owner.is_none_or(|owner| group.owner == owner))
                .map(|(pgid, _)| *pgid)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(unix))]
fn live_group_ids(_owner: Option<u64>) -> Vec<i32> {
    Vec::new()
}

/// The `<task>@<state>` labels of the invocations still running.
#[cfg(unix)]
fn live_invocation_labels() -> Vec<String> {
    LIVE_GROUPS
        .lock()
        .map(|live| live.values().map(|group| group.label.clone()).collect())
        .unwrap_or_default()
}

#[cfg(not(unix))]
fn live_invocation_labels() -> Vec<String> {
    Vec::new()
}

/// What a termination sequence acts on: something to ask, something to wait
/// for, and something to kill when the asking has run out of time.
///
/// The two callers differ only in these three answers. A group's own waiter
/// signals its group and watches its own child; the shutdown guard signals a
/// set of groups it does not own children for and watches the registry
/// instead. The sequence between them — and the grace it honours — is one.
// §FS-rhei-run.3.2: one termination sequence, whatever triggered it.
trait TerminationTarget {
    /// Ask it to stop.
    fn ask_to_stop(&mut self);
    /// Whether there is nothing left to wait for.
    fn is_gone(&mut self) -> std::io::Result<bool>;
    /// End it now.
    fn kill(&mut self);
}

/// The one early-termination sequence: `SIGTERM`, the 10 s grace, then
/// `SIGKILL` on whatever is left — unless the operator already asked twice,
/// which skips straight to the kill.
// §FS-rhei-run.3.2
fn run_termination_sequence(
    target: &mut dyn TerminationTarget,
    stop: &StopToken,
) -> std::io::Result<()> {
    if !stop.skip_grace() {
        target.ask_to_stop();
        let deadline = Instant::now() + SUPERVISED_TERMINATE_GRACE;
        while Instant::now() < deadline {
            if target.is_gone()? {
                return Ok(());
            }
            // A second interrupt mid-grace is the operator saying "now".
            if stop.skip_grace() {
                break;
            }
            std::thread::sleep(SUPERVISED_GRACE_POLL_INTERVAL);
        }
    }
    target.kill();
    Ok(())
}

/// The registry's side of the sequence: signal a run's groups, and watch for
/// their owning threads to reap them.
#[cfg(unix)]
struct LiveGroupsTarget {
    owner: Option<u64>,
}

#[cfg(unix)]
impl TerminationTarget for LiveGroupsTarget {
    fn ask_to_stop(&mut self) {
        for pgid in live_group_ids(self.owner) {
            // A group its own waiter has already asked is not asked twice.
            if claim_group_termination(pgid) {
                let _ = signal::killpg(Pid::from_raw(pgid), Signal::SIGTERM);
            }
        }
    }

    fn is_gone(&mut self) -> std::io::Result<bool> {
        Ok(live_group_ids(self.owner).is_empty())
    }

    fn kill(&mut self) {
        for pgid in live_group_ids(self.owner) {
            let _ = signal::killpg(Pid::from_raw(pgid), Signal::SIGKILL);
        }
    }
}

/// Terminate live groups with the shared sequence. `owner` scopes it to one
/// run's own groups; `None` takes every registered group. Signals only — each
/// group's owning thread reaps its own child.
// §FS-rhei-run.3.2: one termination sequence, whatever triggered it.
#[cfg(unix)]
fn terminate_live_groups(owner: Option<u64>) {
    if live_group_ids(owner).is_empty() {
        return;
    }
    let _ = run_termination_sequence(&mut LiveGroupsTarget { owner }, &INTERRUPT);
}

#[cfg(not(unix))]
fn terminate_live_groups(_owner: Option<u64>) {}

/// Terminate **every** live group, whoever started it.
///
/// For the one exit a destructor cannot reach: the process is leaving from
/// inside a failed print, through `std::process::exit`, which runs no `Drop`.
/// [`RunSubprocessGuard`] never gets its turn, so this stands in for it — and
/// it cannot ask which run owns what, because the thread that lost its output
/// need not be the thread that started the work.
// §FS-rhei-run.3.2: a run that loses its console ends its groups first.
fn terminate_all_live_groups() {
    // Stop the waits first: a worker that is about to re-enter its own
    // termination sequence should find the run already shutting down.
    INTERRUPT.request();
    terminate_live_groups(None);
}

/// Declared alongside [`RunReportGuard`] so it runs on **every** way out of an
/// execution mode — an early `?`, a panic unwind, or a normal end. Without it,
/// an error return after workers were spawned leaves exactly the orphans this
/// design exists to prevent.
///
/// It claims only the subprocesses of its own run: the registry is global, but
/// ownership is not, so the guard cannot reach into work it did not start.
// §FS-rhei-run.3.2: the supervisor's death ends its subprocesses.
struct RunSubprocessGuard {
    owner: u64,
}

impl RunSubprocessGuard {
    fn install() -> Self {
        let owner = NEXT_RUN_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        set_run_owner(owner);
        Self { owner }
    }
}

impl Drop for RunSubprocessGuard {
    fn drop(&mut self) {
        set_run_owner(0);
        if !live_group_ids(Some(self.owner)).is_empty() {
            // Stop the waits first, or a worker would go on waiting out a
            // deadline against a group this one is already tearing down.
            INTERRUPT.request();
            terminate_live_groups(Some(self.owner));
        }
        // This run is over either way, so whatever it raised on its way out
        // stops here rather than following the process into the next run.
        // §FS-rhei-run.3.2
        INTERRUPT.release_teardown();
    }
}

// ---------------------------------------------------------------------------
// Invocation outcomes
// ---------------------------------------------------------------------------

/// What the run surfaces need from a finished invocation, whichever kind of
/// subprocess did the work. Agents and programs answer it identically, so they
/// are classified by one routine rather than two that have to be kept in step.
trait InvocationOutcome {
    fn was_interrupted(&self) -> bool;
    fn timed_out(&self) -> bool;
    fn status(&self) -> std::process::ExitStatus;
}

/// The slot outcome and exit code an invocation reports to the run surfaces.
///
/// Interruption is tested **first**, before success: the engine ended this
/// invocation, so whatever status it managed to exit with during the grace is
/// not a verdict on the ticket, and reporting it as completed would fire a
/// transition the operator never asked for.
// §FS-rhei-run.3.2: interruption is not a completion.
fn slot_outcome<T: InvocationOutcome>(
    result: &MietteResult<T>,
) -> (rhei_tui::TaskOutcome, Option<i32>) {
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(err) => return (rhei_tui::TaskOutcome::Failed(err.to_string()), None),
    };
    let code = outcome.status().code();
    let reported = if outcome.was_interrupted() {
        rhei_tui::TaskOutcome::Interrupted
    } else if outcome.status().success() {
        rhei_tui::TaskOutcome::Completed
    } else if outcome.timed_out() {
        rhei_tui::TaskOutcome::TimedOut
    } else {
        rhei_tui::TaskOutcome::Failed(format!("exit {}", code.unwrap_or(-1)))
    };
    (reported, code)
}

/// The operator-facing line for an invocation the run shut down, with the log
/// named when there is one. One wording for every path that can print it: a
/// ticket that reads one way in the sequential run and another in the parallel
/// one reads as two different situations.
// §FS-rhei-run.3.2
fn interrupted_task_warning(task_id: &str, state: &str, log: Option<&Path>) -> String {
    let head = format!("  Task {task_id} interrupted in '{state}'; state unchanged.");
    match log {
        Some(log) => format!("{head} Log: {}", log.display()),
        None => head,
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
                    // OOM — where the supervisor runs no code. §FS-rhei-run.3.2
                    let _ = nix::sys::prctl::set_pdeathsig(Some(Signal::SIGTERM));
                    // The per-thread semantics are harmless: the thread that
                    // spawns a subprocess is the one that waits on it, so it
                    // outlives the child on every path short of the whole
                    // process dying. Arming happens after the fork, though, so
                    // a supervisor that died in between would never deliver it
                    // — close that window before going any further.
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
    ///
    /// `notify` is handed the shutdown notice by whichever waiter notices the
    /// interrupt first, and by no other; it is the caller's to supply because
    /// only the caller knows which sink the operator is reading.
    // §FS-rhei-run.3.2: timeout and shutdown are one routine.
    fn wait(
        &mut self,
        timeout: Option<Duration>,
        stop: &StopToken,
        notify: &dyn Fn(String),
    ) -> std::io::Result<Ended> {
        let start = Instant::now();
        let mut poll = SUPERVISED_POLL_MIN;
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.finish();
                return Ok(Ended { status, cause: EndCause::Exited });
            }
            // Shutdown outranks the deadline: both are true when an agent is
            // seconds from its timeout as the operator hits Ctrl+C, and calling
            // that a timeout fires a transition. §FS-rhei-run.3.2
            let interrupted = stop.is_set();
            let timed_out = !interrupted && timeout.is_some_and(|limit| start.elapsed() > limit);
            if interrupted || timed_out {
                let cause = if interrupted {
                    // The first waiter to notice names every invocation the
                    // shutdown is about to end. §FS-rhei-run.3.2
                    if let Some(notice) = stop.take_announcement() {
                        notify(notice);
                    }
                    EndCause::Interrupted
                } else {
                    EndCause::TimedOut
                };
                let status = self.terminate_and_reap(stop)?;
                return Ok(Ended { status, cause });
            }
            std::thread::sleep(poll);
            poll = next_poll_interval(poll);
        }
    }

    /// Run the shared termination sequence against this invocation's group and
    /// reap its leader. §FS-rhei-run.3.2
    fn terminate_and_reap(
        &mut self,
        stop: &StopToken,
    ) -> std::io::Result<std::process::ExitStatus> {
        run_termination_sequence(self, stop)?;
        let status = match self.child.try_wait()? {
            Some(status) => status,
            // Either the grace ran out and the group was killed, or it ended
            // between the last poll and here; `wait` settles both.
            None => self.child.wait()?,
        };
        self.finish();
        Ok(status)
    }

    /// Record that the child has been reaped and drop the group's registration.
    fn finish(&mut self) {
        self.reaped = true;
        #[cfg(unix)]
        unregister_live_group(self.pgid);
    }

    /// Ask the whole group to stop, unless the shutdown guard already has.
    #[cfg(unix)]
    fn terminate_group(&mut self) {
        if claim_group_termination(self.pgid) {
            let _ = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGTERM);
        }
    }

    /// Windows has no process group to signal here, so the direct child is
    /// killed exactly as it was before this change. §FS-rhei-run.3.2
    #[cfg(not(unix))]
    fn terminate_group(&mut self) {
        let _ = self.child.kill();
    }

    /// End the whole group now.
    ///
    /// The leader is killed by name as well as by group. It is normally in the
    /// group and gets the `killpg` like everything else; a leader that called
    /// `setsid` would not be, and the `wait()` that follows this would then
    /// have nothing to wait for but a process nobody signalled.
    #[cfg(unix)]
    fn kill_group(&mut self) {
        let _ = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGKILL);
        let _ = self.child.kill();
    }

    #[cfg(not(unix))]
    fn kill_group(&mut self) {
        let _ = self.child.kill();
    }
}

/// This invocation's side of the sequence: signal its own group, and watch its
/// own child.
impl TerminationTarget for Supervised {
    fn ask_to_stop(&mut self) {
        self.terminate_group();
    }

    fn is_gone(&mut self) -> std::io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    fn kill(&mut self) {
        self.kill_group();
    }
}

impl Drop for Supervised {
    fn drop(&mut self) {
        if !self.reaped {
            // Left by an error or a panic before the wait finished. It gets
            // the same sequence as every other early termination: leaving by an
            // error earns this group no less time to flush. §FS-rhei-run.3.2
            let _ = self.terminate_and_reap(&INTERRUPT);
        }
        #[cfg(unix)]
        unregister_live_group(self.pgid);
    }
}
