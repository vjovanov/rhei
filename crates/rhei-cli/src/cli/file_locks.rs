// The plan-file lock every rewriting command takes, and the one classification
// every `fs2` try-lock in this crate depends on.
//
// Its own part because both are platform facts rather than command behavior,
// and the commands that got them wrong — `rhei complete`, `rhei transition`,
// `rhei reset`, a dashboard gate choice, the run-lock liveness probe, the
// headless launch lock, snapshot-continue — had each gone their own way.

// §AR-source-file-size.3

/// A plan file held under an exclusive lock for the length of one rewrite.
///
/// It owns the handle rather than borrowing it, because releasing the lock and
/// closing the handle are the same act on Windows: the lock there is a
/// mandatory byte range, and the filesystem refuses to hand the path to a new
/// file while the old one is open and locked. A rewrite that renames a temp
/// file over its own locked plan has to let go first.
struct LockedPlanFile {
    /// `None` once released. A refused replace releases it early and the caller
    /// releases it again on its way out, so taking it has to be idempotent.
    file: std::cell::RefCell<Option<fs::File>>,
    path: PathBuf,
}

impl LockedPlanFile {
    /// Open `path` and take its exclusive lock, blocking until it is free.
    fn open(path: &Path) -> MietteResult<Self> {
        let file = fs::File::open(path)
            .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
        file.lock_exclusive()
            .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;
        Ok(Self { file: std::cell::RefCell::new(Some(file)), path: path.to_path_buf() })
    }

    /// Read the locked file, by path first.
    ///
    /// That ordering is the point: every plan rewrite in rhei replaces the file
    /// by renaming a temp file over it, so a writer that took the lock before
    /// us has left a *different* file at `path` and this handle still names the
    /// one it replaced. Reading by path is what makes the lock protect content
    /// rather than merely serialize.
    ///
    /// The fallback is for Windows, where a second handle onto a file *this*
    /// process has locked is refused outright — the writer locks the plan and
    /// then cannot read it. That refusal is the one case in which the handle is
    /// known to still be the file at `path`: had anybody replaced it, the
    /// replacement would carry no lock and the read would have succeeded.
    ///
    /// `action` names the read the way `file_io_report` wants it, so a caller's
    /// diagnostic reads the same as it did when this was `fs::read_to_string`.
    fn read_to_string(&self, action: &str) -> MietteResult<String> {
        let err = match fs::read_to_string(&self.path) {
            Ok(raw) => return Ok(raw),
            Err(err) => err,
        };
        if !lock_is_contended(&err) {
            return Err(file_io_report(&self.path, action, err));
        }
        let borrowed = self.file.borrow();
        let Some(handle) = borrowed.as_ref() else {
            return Err(file_io_report(&self.path, action, err));
        };
        let mut reader = handle;
        reader
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|err| file_io_report(&self.path, action, err))?;
        let mut raw = String::new();
        reader.read_to_string(&mut raw).map_err(|err| file_io_report(&self.path, action, err))?;
        Ok(raw)
    }

    /// Release the lock and close the handle. Idempotent, and a no-op once a
    /// refused replace has already done it.
    fn release(&self) {
        if let Some(file) = self.file.borrow_mut().take() {
            let _ = fs2::FileExt::unlock(&file);
        }
    }
}

/// Rename a temp file over `path`, which `locked` may be holding.
///
/// A refused replace releases the lock and tries once more — exactly once; a
/// second refusal is returned to the caller. Unix never reaches the retry, an
/// advisory lock refusing no rename, so everything below is about Windows.
///
/// **This opens a window, and the window is real.** Between the `release()` and
/// the rename that follows it, the plan is held by nobody and this process has
/// no handle on it: a command that was blocked on the lock acquires it there,
/// reads the file as it stood *before* our rename, and may persist its own
/// rewrite after ours — losing ours entirely. Nothing here narrows that; the
/// retry only makes the write possible at all, where the alternative is a
/// rewrite that cannot land on Windows even uncontended.
///
/// Nor does the caller's own `release()` afterwards close it: by then the lock
/// object names a file no path points at any more, so releasing it is letting
/// go of an orphan, and whatever the caller does between the rename and that
/// release — an `on_enter` callback, a rollback write — it does unlocked.
///
/// The fix is a lock that does not live on the file being replaced: a sidecar
/// the rename never touches, held across the whole rewrite. That is #95, and it
/// is a change to every locking command rather than to this function.
fn persist_locked(
    tmp: tempfile::NamedTempFile,
    path: &Path,
    locked: Option<&LockedPlanFile>,
) -> Result<(), tempfile::PersistError> {
    let refused = match tmp.persist(path) {
        Ok(_) => return Ok(()),
        Err(refused) => refused,
    };
    let Some(locked) = locked else {
        return Err(refused);
    };
    locked.release();
    refused.file.persist(path).map(|_| ())
}

/// Whether a failed try-lock — or a read a lock refused — means *somebody
/// already holds it*, as opposed to failing for a reason that says nothing
/// about the holder.
///
/// The platforms disagree on the errno: Unix refuses a contended `flock` with
/// `EWOULDBLOCK`, Windows refuses a contended `LockFileEx` with
/// `ERROR_LOCK_VIOLATION` (os error 33), which is not `WouldBlock`. Classifying
/// only `WouldBlock` as held reported every live Windows run as *unknown*.
/// `fs2::lock_contended_error()` is the platform's own answer, so both spell
/// the same verdict here.
// §FS-rhei-run-headless.3
fn lock_is_contended(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    match (err.raw_os_error(), fs2::lock_contended_error().raw_os_error()) {
        (Some(observed), Some(contended)) => observed == contended,
        _ => false,
    }
}
