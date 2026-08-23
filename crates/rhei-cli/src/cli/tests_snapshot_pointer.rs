// The two spellings of a snapshot identity's `current` pointer, written and
// read back. The regular-file writer is Windows' path in production, and these
// exercise it on Linux, where the whole suite runs.

// §FS-rhei-snapshots.7 §FS-rhei-snapshots.7.2

mod snapshot_pointer_tests {
    use super::super::*;

    /// Every entry in an identity directory, so a test can say what the write
    /// left behind rather than only what it produced.
    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read identity dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn file_pointer_names_the_generation_and_leaves_no_temp_behind() {
        let dir = tempfile::tempdir().expect("tmpdir");
        write_current_pointer_file(dir.path(), "g1", "abc123").expect("write pointer");

        assert_eq!(entries(dir.path()), vec!["current".to_string()]);
        let raw = fs::read_to_string(dir.path().join("current")).expect("read pointer");
        assert_eq!(raw.trim(), "g1");
    }

    #[test]
    fn file_pointer_replaces_the_previous_generation() {
        let dir = tempfile::tempdir().expect("tmpdir");
        write_current_pointer_file(dir.path(), "g1", "first").expect("write pointer");
        write_current_pointer_file(dir.path(), "g2", "second").expect("advance pointer");

        // The advance renames over an existing `current`; a writer that could
        // not replace one would fail here rather than move the pointer.
        assert_eq!(entries(dir.path()), vec!["current".to_string()]);
        assert_eq!(snapshot_current_target(dir.path()), Some(PathBuf::from("g2")));
    }

    #[test]
    fn file_pointer_write_reuses_a_temp_left_by_an_interrupted_attempt() {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(dir.path().join("current.tmp-abc123"), "g9\n").expect("stale temp");

        write_current_pointer_file(dir.path(), "g1", "abc123").expect("write pointer");

        assert_eq!(entries(dir.path()), vec!["current".to_string()]);
        assert_eq!(snapshot_current_target(dir.path()), Some(PathBuf::from("g1")));
    }

    #[test]
    fn reader_follows_a_regular_file_pointer() {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::create_dir(dir.path().join("g1")).expect("generation dir");
        fs::write(dir.path().join("current"), "g1\n").expect("pointer");

        assert_eq!(snapshot_current_target(dir.path()), Some(PathBuf::from("g1")));
        assert!(snapshot_current_points_to(&dir.path().join("g1")));
    }

    #[cfg(unix)]
    #[test]
    fn reader_follows_a_symlink_pointer() {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::create_dir(dir.path().join("g3")).expect("generation dir");
        replace_current_pointer(dir.path(), "g3", "nonce").expect("write pointer");

        assert!(dir.path().join("current").is_symlink(), "unix writes the symlink spelling");
        assert_eq!(snapshot_current_target(dir.path()), Some(PathBuf::from("g3")));
        assert!(snapshot_current_points_to(&dir.path().join("g3")));
    }

    #[test]
    fn reader_answers_nothing_for_a_missing_or_blank_pointer() {
        let dir = tempfile::tempdir().expect("tmpdir");
        assert_eq!(snapshot_current_target(dir.path()), None);

        fs::write(dir.path().join("current"), "  \n").expect("blank pointer");
        assert_eq!(snapshot_current_target(dir.path()), None);
    }
}
