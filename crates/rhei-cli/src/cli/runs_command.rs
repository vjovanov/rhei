// `rhei runs` and `rhei stop`: see what is live on this machine, and ask one of
// them to stop.
//
// Stopping is a signal, not a route. The loopback control server keeps the
// single inbound mutation boundary it was designed around, and stopping
// inherits the run's interruption contract whole instead of growing a second
// teardown.

// §FS-rhei-run.3.2 §FS-rhei-run-headless.6 §FS-rhei-run-headless.7
// §DA-detached-runs

/// How long a `--kill` waits between asking once and asking again. The run's
/// own handler escalates on the second signal, so this is literally the
/// operator asking twice — not a different mechanism. §FS-rhei-run.3.2
const KILL_ESCALATION_GRACE: Duration = Duration::from_secs(2);

/// How often `--wait` re-checks whether the run has actually gone.
const WAIT_POLL: Duration = Duration::from_millis(200);

/// List the runs that are live on this machine. §FS-rhei-run-headless.6
pub(crate) fn runs_command(json: bool) -> MietteResult<()> {
    let sweep = sweep_run_registry();
    if json {
        let rendered = serde_json::to_string_pretty(&sweep.live).map_err(|err| {
            miette!(
                help = "read the run list as text instead: rhei runs",
                "could not render the run list as JSON: {err}"
            )
        })?;
        println!("{rendered}");
        // Stdout is the array and nothing else, so what could not be decided
        // is said on stderr rather than silently dropped.
        for entry in &sweep.undecided {
            eprintln!("warning: could not check {}: {}", entry.summary_line(), entry.reason);
        }
        return Ok(());
    }
    // An empty list is an answer, not a failure: nothing running is the normal
    // state of a machine. §FS-rhei-run-headless.6
    if sweep.live.is_empty() {
        println!("No runs are live on this machine.");
        report_undecided_runs(&sweep.undecided);
        println!("Start one with: rhei run --headless <plan>");
        return Ok(());
    }
    let live = sweep.live.len();
    println!("{live} live run{}:", if live == 1 { "" } else { "s" });
    for run in &sweep.live {
        println!("  {}", run.summary_line());
        println!("      started {}  {}", run.started_at, run.workspace.display());
        if let Some(url) = &run.control_url {
            // The **control** URL, not "the dashboard": under `--no-dashboard`
            // nothing may invite a browser, and the endpoints an attached
            // surface needs are up either way. §FS-rhei-run-headless.4
            println!("      control   {url}");
        }
    }
    report_undecided_runs(&sweep.undecided);
    println!();
    println!("Attach to one with: rhei attach <id>");
    Ok(())
}

/// Say what could not be checked, and why.
///
/// Silently keeping these and silently omitting them from the listing is the
/// same lie by a different route: the operator reads "no runs are live" for a
/// machine whose registry this process simply could not read.
// §FS-rhei-run-headless.3 §FS-rhei-run-headless.6
fn report_undecided_runs(entries: &[UndecidedRun]) {
    if entries.is_empty() {
        return;
    }
    println!();
    println!("{} entr{} could not be checked:", entries.len(), if entries.len() == 1 { "y" } else { "ies" });
    for entry in entries {
        println!("  {}", entry.summary_line());
        println!("      {}", entry.reason);
    }
    println!("  Kept: an unreadable file says nothing about the process it describes.");
}

/// Ask a run to stop. §FS-rhei-run-headless.7
pub(crate) fn stop_command(reference: Option<&str>, kill: bool, wait: bool) -> MietteResult<()> {
    let descriptor = resolve_run(reference)?;
    // Stopping something that has already stopped is not an error: the
    // operator's intent — "make sure this is not running" — is satisfied.
    //
    // Only a *decided* end short-circuits, though. An entry this process could
    // not check is not an entry to report as ended: the operator asked to make
    // sure the run is not running, and a `SIGINT` to a pid that is gone is a
    // harmless `ESRCH`.

    // §FS-rhei-run-headless.3 §FS-rhei-run-headless.7
    match descriptor.liveness() {
        Liveness::Ended | Liveness::Gone => {
            println!("Run {} has already ended.", descriptor.id);
            report_recorded_result(&descriptor);
            return Ok(());
        }
        Liveness::Live => {}
        Liveness::Unknown(reason) => {
            eprintln!(
                "warning: could not confirm whether run {} is still running ({reason}); \
                 signalling pid {} anyway",
                descriptor.id, descriptor.pid
            );
        }
    }

    confirm_signal_target(&descriptor)?;
    signal_run(&descriptor, "stop")?;
    println!("Asked run {} (pid {}) to stop.", descriptor.id, descriptor.pid);
    if kill {
        // The run is owed the grace its first signal opened before the second
        // one takes it away. §FS-rhei-run.3.2
        std::thread::sleep(KILL_ESCALATION_GRACE);
        // Undecided is not ended, so it does not skip the escalation the
        // operator asked for. §FS-rhei-run-headless.3
        if !descriptor.liveness().has_ended() {
            confirm_signal_target(&descriptor)?;
            signal_run(&descriptor, "escalate")?;
            println!("Asked again — in-flight work is being killed without its grace.");
        }
    }

    if wait {
        await_run_end(&descriptor)?;
        println!("Run {} has ended.", descriptor.id);
        // Re-read: the run rewrote its descriptor on the way out.
        let final_state = read_descriptor(&run_descriptor_path(&descriptor.workspace));
        report_recorded_result(final_state.as_ref().unwrap_or(&descriptor));
    } else {
        println!("It is terminating its in-flight work; `rhei runs` shows when it is gone.");
    }
    Ok(())
}

/// Block until the run has actually gone, not merely until it said it would go.
///
/// An undecided probe keeps the wait going: returning on one reported a run as
/// ended while its process was still tearing down its in-flight work. The grace
/// of [`UndecidedWatch`] bounds it, and running out is an error rather than a
/// quiet "it has ended".
// §FS-rhei-run-headless.7 §FS-rhei-run-headless.3
fn await_run_end(descriptor: &RunDescriptor) -> MietteResult<()> {
    let mut undecided = UndecidedWatch::default();
    loop {
        match descriptor.liveness() {
            Liveness::Ended | Liveness::Gone => return Ok(()),
            Liveness::Live => undecided.decided(),
            Liveness::Unknown(reason) => {
                if undecided.exhausted(&reason) {
                    return Err(miette!(
                        help = "the signal was delivered; check the run's own workspace for \
                                what it recorded",
                        "asked run {} to stop, but could not confirm it ended: {}",
                        descriptor.id,
                        undecided.reason()
                    ));
                }
            }
        }
        std::thread::sleep(WAIT_POLL);
    }
}

fn report_recorded_result(descriptor: &RunDescriptor) {
    match descriptor.exit_code {
        Some(0) => println!("  It exited 0."),
        Some(code) => println!("  It exited {code}."),
        // A `SIGKILL`ed run never got to record one. Say so rather than
        // inventing a status the run did not report. §FS-rhei-run-headless.2
        None => println!("  It recorded no exit status."),
    }
    let report = descriptor.workspace.join("runtime/run-report.md");
    if report.is_file() {
        println!("  Report: {}", report.display());
    }
}

/// Re-read the workspace descriptor immediately before signalling, and refuse
/// unless it still names this run *and* this pid.
///
/// A registry entry is a memory of a pid, and pids are reused. Between
/// resolving the run and delivering the signal the process may have died and
/// its pid been handed to something else entirely — so the authoritative copy
/// gets the last word. An unreadable descriptor is not a refusal: it is the
/// one case where the operator's "make sure this is not running" outranks a
/// check that cannot be performed.
// §FS-rhei-run-headless.7 §FS-rhei-run-headless.3
fn confirm_signal_target(descriptor: &RunDescriptor) -> MietteResult<()> {
    let path = run_descriptor_path(&descriptor.workspace);
    match read_descriptor_result(&path) {
        DescriptorRead::Loaded(current)
            if current.id == descriptor.id && current.pid == descriptor.pid =>
        {
            Ok(())
        }
        DescriptorRead::Loaded(current) => Err(miette!(
            help = "see what is live on this machine with: rhei runs",
            "run {} is no longer the run on {}: it now holds run {} (pid {})",
            descriptor.id,
            descriptor.workspace.display(),
            current.id,
            current.pid
        )),
        DescriptorRead::Missing => Err(miette!(
            help = "see what is live on this machine with: rhei runs",
            "run {} left no descriptor at {}, so there is nothing to confirm its pid against",
            descriptor.id,
            path.display()
        )),
        DescriptorRead::Unreadable(why) => {
            eprintln!(
                "warning: could not re-read {} ({why}); signalling pid {} on the registry's \
                 word alone",
                path.display(),
                descriptor.pid
            );
            Ok(())
        }
    }
}

/// Deliver `SIGINT` to the run, entering §FS-rhei-run.3.2 exactly as an
/// operator's Ctrl+C does.
#[cfg(unix)]
fn signal_run(descriptor: &RunDescriptor, what: &str) -> MietteResult<()> {
    let pid = Pid::from_raw(descriptor.pid as i32);
    signal::kill(pid, Signal::SIGINT).map_err(|err| {
        miette!(
            help = "the run may have ended already; check `rhei runs`",
            "could not {what} run {} (pid {}): {err}",
            descriptor.id,
            descriptor.pid
        )
    })
}

#[cfg(not(unix))]
fn signal_run(descriptor: &RunDescriptor, _what: &str) -> MietteResult<()> {
    Err(miette!(
        help = "stop the run from the terminal that is running it",
        "`rhei stop` needs POSIX signals and is not supported on this platform yet \
         (run {} is pid {})",
        descriptor.id,
        descriptor.pid
    ))
}
