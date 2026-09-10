// Linux stop must distinguish a missing process from an unavailable process
// handle before taking the already-ended success path. §FS-rhei-run-headless.7

#[cfg(target_os = "linux")]
mod stop_pidfd_tests {
    use super::run_descriptor_tests::{descriptor, workspace};
    use super::super::*;

    #[test]
    fn esrch_decides_that_the_recorded_process_is_absent() {
        let workspace = workspace();
        let run = descriptor("gone01", &workspace.path, "2026-09-10T15:00:00Z");

        let live = pidfd_open_result_has_live_recorded_process::<()>(
            &run,
            Err(rustix::io::Errno::SRCH),
        )
        .expect("ESRCH is a decided absence");

        assert!(!live);
    }

    #[test]
    fn enosys_refuses_to_decide_that_the_recorded_process_is_absent() {
        let workspace = workspace();
        let run = descriptor("unknown1", &workspace.path, "2026-09-10T15:00:00Z");

        let error = pidfd_open_result_has_live_recorded_process::<()>(
            &run,
            Err(rustix::io::Errno::NOSYS),
        )
        .expect_err("ENOSYS must not become an already-ended success");

        assert!(error.to_string().contains("could not be checked safely"), "got: {error}");
    }
}
