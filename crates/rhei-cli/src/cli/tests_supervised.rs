    /// Unit tests for the stop token and the supervised wait routine.
    ///
    /// The token is exercised as a value, never through the process-wide
    /// `INTERRUPT`: these tests run concurrently with tests that drive whole
    /// runs, and raising the global token would stop them too.
    // §FS-rhei-run.3.2 §DA-supervised-process-groups
    #[cfg(unix)]
    mod supervised_tests {
        use super::*;
        use std::os::unix::process::ExitStatusExt as _;

        /// Long enough that nothing under test can outlive the assertion by
        /// finishing on its own.
        const CHILD_SLEEP: &str = "sleep 30";

        /// The tests assert on causes, not on operator-facing text; the
        /// notice's routing is the frontend's test. §FS-rhei-run-tui.1.8
        fn ignore_notice(_: String) {}

        fn sh(script: &str) -> std::process::Command {
            let mut cmd = std::process::Command::new("sh");
            cmd.arg("-c").arg(script);
            cmd.stdin(std::process::Stdio::null());
            cmd
        }

        /// Whether a pid still exists, by the `kill -0` rule.
        fn pid_is_alive(pid: i32) -> bool {
            signal::kill(Pid::from_raw(pid), None).is_ok()
        }

        /// Poll until `check` holds or the deadline passes; no fixed sleeps, so
        /// a slow machine costs patience rather than a failure.
        fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if check() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("timed out waiting for {what}");
        }

        #[test]
        fn a_fresh_token_is_not_set() {
            let token = StopToken::new();
            assert!(!token.is_set());
            assert!(!token.skip_grace());
            assert_eq!(token.level(), 0);
            assert_eq!(token.exit_code(), None);
        }

        #[test]
        fn the_first_signal_wins_and_names_the_exit_code() {
            let token = StopToken::new();
            token.raise(Signal::SIGINT as i32);
            token.raise(Signal::SIGTERM as i32);
            assert_eq!(token.signal_number(), Some(Signal::SIGINT as i32));
            // 128 + SIGINT: what a shell reports for a process SIGINT killed.
            assert_eq!(token.exit_code(), Some(130));
        }

        /// A second interrupt is the operator saying "now", so it and nothing
        /// less flips the skip-grace decision.
        #[test]
        fn a_second_interrupt_skips_the_grace_and_a_first_does_not() {
            let token = StopToken::new();
            token.raise(Signal::SIGINT as i32);
            assert!(token.is_set());
            assert!(!token.skip_grace(), "one Ctrl+C asks; it does not kill");
            token.raise(Signal::SIGINT as i32);
            assert!(token.skip_grace());
        }

        /// The shutdown guard stops in-flight work without naming a signal, so
        /// the exit code stays whatever the run itself decided.
        #[test]
        fn a_requested_stop_sets_no_exit_code() {
            let token = StopToken::new();
            token.request();
            assert!(token.is_set());
            assert_eq!(token.exit_code(), None);
        }

        /// A wedged operator leaning on Ctrl+C must not wrap the counter back
        /// round to "not interrupted".
        #[test]
        fn the_level_saturates_instead_of_wrapping() {
            let token = StopToken::new();
            for _ in 0..300 {
                token.request();
            }
            assert_eq!(token.level(), u8::MAX);
            assert!(token.is_set());
            assert!(token.skip_grace());
        }

        /// A subprocess that finishes fast must be noticed fast; one that runs
        /// for minutes must not be polled at 10 ms for all of them. The ramp is
        /// what lets one wait routine serve a redactor and an agent alike.
        #[test]
        fn the_poll_interval_ramps_from_the_floor_to_the_cap_and_stops() {
            assert_eq!(next_poll_interval(SUPERVISED_POLL_MIN), Duration::from_millis(20));
            let mut poll = SUPERVISED_POLL_MIN;
            for _ in 0..10 {
                poll = next_poll_interval(poll);
            }
            assert_eq!(poll, SUPERVISED_POLL_MAX, "the ramp settles at the cap");
            assert_eq!(next_poll_interval(SUPERVISED_POLL_MAX), SUPERVISED_POLL_MAX);
        }

        #[test]
        fn a_subprocess_that_exits_on_its_own_reports_exited() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh("exit 3"), "unit@exit").expect("spawn");
            let ended = supervised.wait(Some(Duration::from_secs(30)), &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Exited);
            assert_eq!(ended.status.code(), Some(3));
        }

        #[test]
        fn a_deadline_reports_timed_out_and_ends_the_group() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@timeout").expect("spawn");
            let pgid = supervised.pgid;
            let ended =
                supervised.wait(Some(Duration::from_millis(1)), &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::TimedOut);
            assert!(!ended.status.success());
            drop(supervised);
            wait_until("the timed-out group to be gone", || !pid_is_alive(pgid));
        }

        /// The token, not only the deadline, ends a wait — and a supervised
        /// invocation with no timeout at all is still interruptible, which the
        /// old blocking `wait()` was not.
        #[test]
        fn a_raised_token_reports_interrupted_even_without_a_deadline() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@interrupt").expect("spawn");
            let pgid = supervised.pgid;
            token.raise(Signal::SIGTERM as i32);
            let ended = supervised.wait(None, &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            drop(supervised);
            wait_until("the interrupted group to be gone", || !pid_is_alive(pgid));
        }

        /// The unit `rhei run` owns is the group, not the child: a subprocess
        /// that hands its work to a grandchild cannot outlive its own death
        /// certificate.
        #[test]
        fn terminating_a_supervised_invocation_takes_its_grandchildren() {
            let dir = tempfile::tempdir().expect("tmpdir");
            let pid_file = dir.path().join("grandchild.pid");
            let token = StopToken::new();
            let mut supervised = Supervised::spawn(
                &mut sh(&format!(
                    "sleep 30 & echo $! > {}; {CHILD_SLEEP}",
                    pid_file.display()
                )),
                "unit@grandchild",
            )
            .expect("spawn");
            let pgid = supervised.pgid;

            wait_until("the grandchild to record its pid", || {
                fs::read_to_string(&pid_file).map(|text| !text.trim().is_empty()).unwrap_or(false)
            });
            let grandchild: i32 = fs::read_to_string(&pid_file)
                .expect("read pid file")
                .trim()
                .parse()
                .expect("grandchild pid");
            assert!(pid_is_alive(grandchild), "the grandchild should be running");

            token.raise(Signal::SIGTERM as i32);
            let ended = supervised.wait(None, &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            drop(supervised);

            wait_until("the leader to be gone", || !pid_is_alive(pgid));
            wait_until("the grandchild to be gone", || !pid_is_alive(grandchild));
        }

        /// A `Supervised` dropped without being waited on — an error return, a
        /// panic unwind — must not leave its group running.
        #[test]
        fn dropping_an_unwaited_invocation_kills_its_group() {
            let supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@drop").expect("spawn");
            let pgid = supervised.pgid;
            assert!(pid_is_alive(pgid));
            drop(supervised);
            wait_until("the dropped group to be gone", || !pid_is_alive(pgid));
        }

        /// The registry is what the shutdown guard reads, so a reaped
        /// invocation must leave it — otherwise the guard waits out a grace for
        /// a process that is already gone.
        #[test]
        fn a_reaped_invocation_leaves_the_live_registry() {
            let token = StopToken::new();
            let mut supervised =
                Supervised::spawn(&mut sh("exit 0"), "unit@registry").expect("spawn");
            let pgid = supervised.pgid;
            assert!(live_group_ids_for_test().contains(&pgid));
            supervised.wait(Some(Duration::from_secs(30)), &token, &ignore_notice).expect("wait");
            assert!(!live_group_ids_for_test().contains(&pgid));
        }

        /// Every registered group, whoever owns it.
        fn live_group_ids_for_test() -> Vec<i32> {
            LIVE_GROUPS.lock().map(|live| live.keys().copied().collect()).unwrap_or_default()
        }

        /// A group registered by a thread that no run owns is invisible to any
        /// run's guard: ownership is what a guard terminates, not liveness.
        #[test]
        fn an_unowned_group_is_claimed_by_no_run() {
            let supervised =
                Supervised::spawn(&mut sh(CHILD_SLEEP), "unit@unowned").expect("spawn");
            let pgid = supervised.pgid;
            assert_eq!(current_run_owner(), 0, "a test thread runs under no run");
            assert!(!owned_group_ids(1).contains(&pgid));
            assert!(!owned_group_ids(0).is_empty(), "unowned groups are still tracked");
            drop(supervised);
        }

        /// `SIGKILL` is what ends a group that ignores `SIGTERM`; the exit
        /// status reports the signal, which is how the log footer's non-zero
        /// code is explained.
        #[test]
        fn a_group_that_ignores_sigterm_is_killed() {
            let dir = tempfile::tempdir().expect("tmpdir");
            let ready = dir.path().join("trapped");
            let token = StopToken::new();
            let mut supervised = Supervised::spawn(
                &mut sh(&format!(
                    "trap '' TERM; : > {}; {CHILD_SLEEP}",
                    ready.display()
                )),
                "unit@stubborn",
            )
            .expect("spawn");
            // Raising the token before the shell has run `trap` would kill it
            // with the default disposition and prove nothing.
            wait_until("the child to ignore SIGTERM", || ready.exists());
            token.raise(Signal::SIGINT as i32);
            let ended = supervised.wait(None, &token, &ignore_notice).expect("wait");
            assert_eq!(ended.cause, EndCause::Interrupted);
            assert_eq!(
                ended.status.signal(),
                Some(Signal::SIGKILL as i32),
                "the grace expires and the group is killed"
            );
        }
    }

    /// A stdout the process can no longer write to is not a bug in it: a
    /// pipeline's reader closed early, or — under the TUI — the operator's
    /// terminal went away and every later write to the dead pty returns `EIO`.
    /// Left unrecognized, the second one panicked the end-of-run summary and
    /// then panicked again from the report guard while unwinding, which aborts.
    // §FS-rhei-run-tui.1.5.7
    #[test]
    fn a_lost_stdout_is_recognized_by_message_and_by_errno() {
        assert!(message_is_lost_output("failed printing to stdout: Broken pipe (os error 32)"));
        assert!(message_is_lost_output(
            "failed printing to stdout: Input/output error (os error 5)"
        ));
        // The errno alone is enough, so a non-English `strerror` still matches.
        assert!(message_is_lost_output("failed printing to stderr: <translated> (os error 5)"));

        // A different write failure, and a panic that merely mentions one, are
        // both real reports.
        assert!(!message_is_lost_output(
            "failed printing to stdout: No space left on device (os error 28)"
        ));
        assert!(!message_is_lost_output("agent wrote to a Broken pipe (os error 32)"));
    }
