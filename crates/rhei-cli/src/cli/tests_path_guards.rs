// What counts as a rooted path, on the platform running the test. The Windows
// spellings are asserted under `cfg(windows)` because they are not rooted
// anywhere else: `C:out.md` on Linux is an ordinary file name.

// §FS-rhei-states.1.3

mod path_guard_tests {
    use super::super::*;

    #[test]
    fn a_filesystem_root_is_rooted() {
        assert!(path_is_rooted("/etc/passwd"));
        assert!(path_is_rooted("/"));
    }

    #[test]
    fn a_workspace_relative_path_is_not_rooted() {
        assert!(!path_is_rooted("runtime/out.md"));
        assert!(!path_is_rooted("out.md"));
    }

    #[test]
    fn a_curdir_or_climbing_path_is_not_rooted() {
        // Climbing is a different question, asked separately by the callers
        // that care; this guard answers only "does it start at a root".
        assert!(!path_is_rooted("./a"));
        assert!(!path_is_rooted("../a"));
        assert!(!path_is_rooted("../../etc/passwd"));
    }

    #[test]
    fn an_empty_path_is_not_rooted() {
        assert!(!path_is_rooted(""));
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_relative_path_is_rooted() {
        // The case `is_absolute()` and `has_root()` both miss: a `Prefix` with
        // no `RootDir` resolves against the current directory of drive `C:`,
        // which is not the workspace.
        assert!(path_is_rooted("C:out.md"));
        assert!(path_is_rooted(r"C:\out.md"));
        assert!(path_is_rooted(r"\out.md"));
        assert!(path_is_rooted(r"\\server\share\out.md"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_still_reads_a_relative_path_as_relative() {
        assert!(!path_is_rooted(r"runtime\out.md"));
    }

    /// Every canonicalization in the CLI goes through `canonical_path`, which
    /// is `canonicalize` plus `plain_path`. What that buys is asserted on the
    /// pure half, so the platform that runs the whole suite pins the spelling
    /// the other one prints: `rhei diag` names a skill source directory, and it
    /// must name it the way every other line of the report does.
    // §REQ-cross-platform.5
    #[test]
    fn a_printed_skill_source_carries_no_verbatim_prefix() {
        let verbatim = PathBuf::from(r"\\?\C:\rhei\skills\rhei-plan-writer");
        let printed = rhei_core::platform::plain_path(verbatim).display().to_string();
        let expected = if cfg!(windows) {
            r"C:\rhei\skills\rhei-plan-writer"
        } else {
            // Nothing to strip: that is an ordinary relative name here.
            r"\\?\C:\rhei\skills\rhei-plan-writer"
        };
        assert_eq!(printed, expected);

        // And the whole helper on a directory that exists, which is the call
        // `filesystem_skill_source` actually makes.
        let dir = tempfile::tempdir().expect("tmpdir");
        let canonical =
            rhei_core::platform::canonical_path(dir.path()).expect("canonicalize the fixture");
        assert!(
            !canonical.display().to_string().starts_with(r"\\?\"),
            "canonical_path must hand back the plain spelling: {}",
            canonical.display()
        );
    }
}
