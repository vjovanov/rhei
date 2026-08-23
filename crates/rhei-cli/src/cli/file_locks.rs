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
/// A refused replace releases the lock and tries once more. Nothing is lost by
/// releasing it there: the lock lives on the file object, the replace hands
/// `path` to a *different* object, and a caller's own release after this point
/// is already letting go of an orphan. Unix never reaches the retry — an
/// advisory lock refuses no rename — so the window this opens exists only where
/// the alternative is not writing at all.
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
