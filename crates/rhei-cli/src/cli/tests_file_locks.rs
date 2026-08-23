// The two platform facts every rewriting command now shares: what a refused
// lock means, and which of two candidate readings of a locked plan is the
// authoritative one.

// §FS-rhei-run-headless.3 §FS-rhei-new.4

mod file_lock_tests {
    use super::super::*;

    fn plan_file(dir: &tempfile::TempDir, contents: &str) -> PathBuf {
        let path = dir.path().join("plan.rhei.md");
        fs::write(&path, contents).expect("write plan");
        path
    }

    #[test]
    fn a_refused_lock_reads_as_held() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "held\n");

        // Two open file descriptions on one file, from this one process: on
        // Unix that is enough for `flock` to refuse the second, which is the
        // refusal the probe has to classify.
        let holder = fs::File::open(&path).expect("open holder");
        holder.lock_exclusive().expect("hold the lock");
        let contender = fs::File::open(&path).expect("open contender");

        let err = contender.try_lock_exclusive().expect_err("a held lock must refuse");
        assert!(lock_is_contended(&err), "a refused lock is a held lock: {err:?}");

        let _ = fs2::FileExt::unlock(&holder);
    }

    #[test]
    fn an_error_that_is_not_a_refusal_says_nothing_about_a_holder() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let err = fs::File::open(dir.path().join("absent.rhei.md"))
            .expect_err("opening a missing file must fail");

        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(!lock_is_contended(&err), "a missing file names no lock holder");
    }

    #[test]
    fn a_locked_plan_reads_back_while_the_lock_is_held() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "locked\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        // The Windows case, asserted everywhere: a mandatory byte-range lock
        // refuses this process its own read by path, and the read falls back to
        // the handle it is holding rather than failing the command.
        assert_eq!(locked.read_to_string("failed to read plan file").expect("read"), "locked\n");
        locked.release();
    }

    // A rename over a locked file is refused on Windows, so the state this
    // builds — somebody else's file at our locked path — cannot arise there.
    #[cfg(unix)]
    #[test]
    fn a_locked_plan_is_read_by_path_and_not_through_the_handle() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        // What a writer that got there first leaves behind: a *different* file
        // at the same path. The handle still names the one it replaced, so a
        // read through the handle would answer with content nobody can write
        // to any more.
        let replacement = dir.path().join("replacement");
        fs::write(&replacement, "after\n").expect("write replacement");
        fs::rename(&replacement, &path).expect("replace the plan");

        assert_eq!(locked.read_to_string("failed to read plan file").expect("read"), "after\n");
        locked.release();
    }

    #[test]
    fn releasing_a_plan_lock_twice_is_the_same_as_releasing_it_once() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "once\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        locked.release();
        locked.release();

        // A released lock still reads: the path is the source of truth, and the
        // handle was only ever the fallback.
        assert_eq!(locked.read_to_string("failed to read plan file").expect("read"), "once\n");
    }

    #[test]
    fn a_rewrite_persists_over_the_file_it_locked() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = plan_file(&dir, "before\n");
        let locked = LockedPlanFile::open(&path).expect("lock the plan");

        let mut tmp = tempfile::NamedTempFile::new_in(dir.path()).expect("temp file");
        tmp.write_all(b"after\n").expect("write temp");
        persist_locked(tmp, &path, Some(&locked)).expect("persist over the locked plan");
        locked.release();

        assert_eq!(fs::read_to_string(&path).expect("read back"), "after\n");
        let leftovers: Vec<String> = fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name != "plan.rhei.md")
            .collect();
        assert!(leftovers.is_empty(), "the rewrite left {leftovers:?} behind");
    }
}
