// What a detached run's console log promises: emptied for the run that is
// starting, and never rewritten behind a writer that is still going.

// §FS-rhei-run-headless.8

#[cfg(unix)]
mod headless_console_log_tests {
    use super::super::*;

    use std::os::unix::io::AsRawFd as _;

    /// The launcher creates `runtime/` itself, so the helper is only ever asked
    /// for a path whose parent exists.
    fn console_log(dir: &tempfile::TempDir) -> PathBuf {
        let runtime = dir.path().join("runtime");
        fs::create_dir_all(&runtime).expect("runtime directory");
        runtime.join("run.log")
    }

    #[test]
    fn the_console_log_is_emptied_for_the_run_that_opens_it() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = console_log(&dir);
        fs::write(&path, "the previous run's console\n").expect("seed a stale console");

        let mut log = open_console_log(&path).expect("open the console log");
        writeln!(log, "this run").expect("write");

        let contents = fs::read_to_string(&path).expect("read the console log");
        assert_eq!(contents, "this run\n", "one file is one run's console");
    }

    #[test]
    fn the_console_log_is_opened_in_append_mode() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let log = open_console_log(&console_log(&dir)).expect("open the console log");

        let bits = nix::fcntl::fcntl(log.as_raw_fd(), nix::fcntl::FcntlArg::F_GETFL)
            .expect("read the open file description's flags");
        let flags = nix::fcntl::OFlag::from_bits_truncate(bits);
        assert!(flags.contains(nix::fcntl::OFlag::O_APPEND), "the console log must be append-only");
    }

    /// The shape the launch lock does not cover: the pre-check is blind — two
    /// launches on member plans of one root — so a second launcher empties the
    /// console log a live run is still writing to, and the child it starts
    /// renders its run-lock refusal there. Without `O_APPEND` the live run's
    /// next write lands at its own stale offset, in the middle of that
    /// refusal, and cuts the path it names mid-token.
    #[test]
    fn a_second_launchers_truncation_cannot_cut_a_live_runs_diagnostic() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = console_log(&dir);

        let mut live = open_console_log(&path).expect("the live run's console");
        writeln!(live, "Pass 1: 2 ready, 0 terminal, 2 total.").expect("the live run writes");

        let mut loser = open_console_log(&path).expect("the second launcher's console");
        let refusal = "a run is already live on \
                       /private/var/folders/df/T/rhei-integ-headless-childlock-1788072571224528000 \
                       and holds its .rhei/run.lock";
        writeln!(loser, "{refusal}").expect("the losing child renders its refusal");

        writeln!(live, "  Spawning program for Task plan.1: First").expect("the live run writes on");

        let contents = fs::read_to_string(&path).expect("read the console log");
        assert!(contents.contains(refusal), "the refusal was cut mid-token: {contents}");
    }
}
