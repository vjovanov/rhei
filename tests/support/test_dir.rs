// A temporary directory that removes itself.
//
// Shared verbatim by the CLI's two harnesses — one pulls it in with `#[path]`,
// the other with `include!` into its flat module — so the comments here are
// `//` rather than `//!`: an inner doc comment cannot open an included file.

/// A test's own directory, removed when the binding that owns it goes out of
/// scope.
///
/// The explicit `remove_dir_all` at the end of a test only ran when the test
/// reached its end: every failing test left its tree behind, and a suite that
/// creates one directory per test leaves gigabytes there over a few red runs.
/// A guard runs on the unwinding path too, so a failure costs one directory
/// rather than one per attempt. Set `RHEI_KEEP_TEST_DIRS=1` to keep the trees
/// for debugging.
pub struct TestDir {
    path: std::path::PathBuf,
}

impl TestDir {
    /// Create `path` and take ownership of removing it again.
    pub fn create(path: std::path::PathBuf) -> Self {
        std::fs::create_dir_all(&path).expect("temporary directory should be created");
        Self { path }
    }
}

impl std::ops::Deref for TestDir {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.path
    }
}

impl AsRef<std::path::Path> for TestDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl AsRef<std::ffi::OsStr> for TestDir {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.path.as_os_str()
    }
}

impl std::fmt::Debug for TestDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.path, f)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if std::env::var_os("RHEI_KEEP_TEST_DIRS").is_some() {
            return;
        }
        // A test that spawned `rhei` may still be losing the race with the
        // child's exit, and Windows refuses to unlink a file any process still
        // holds open. Retry briefly rather than leaking the tree; never panic,
        // because this runs while a failing test is already unwinding.
        for _ in 0..20 {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
}
