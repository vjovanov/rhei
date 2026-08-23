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
    ///
    /// Shared rather than owned, because the same handle is what
    /// [`HELD_PLAN_LOCKS`] hands to a reader that has no lock object of its own.
    file: PlanLockHandle,
    path: PathBuf,
}

impl LockedPlanFile {
    /// Open `path` and take its exclusive lock, blocking until it is free.
    fn open(path: &Path) -> MietteResult<Self> {
        let file = fs::File::open(path)
            .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
        file.lock_exclusive()
            .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;
        let file = Arc::new(Mutex::new(Some(file)));
        // The loader reads a plan, a workspace index, or a project manifest
        // through `rhei_core::source`, which knows nothing about locks until
        // this process tells it where to ask. §FS-rhei-new.4
        rhei_core::source::set_reader(plan_source_reader);
        held_plan_locks().lock().expect("held plan locks").push((path.to_path_buf(), file.clone()));
        Ok(Self { file, path: path.to_path_buf() })
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
    /// then cannot read it.
    ///
    /// The refusal does not *prove* the handle is still the file at `path`, and
    /// the comment here used to say it did. It is true only while no other
    /// process is inside the release-then-rename window [`persist_locked`]
    /// opens: in that window a rewriter has let go of the file it is about to
    /// replace, so this process can take a lock on a file that is orphaned a
    /// moment later, and a third process locking the *replacement* is enough to
    /// refuse our read by path and send us to a handle naming the old content.
    /// Nothing in this function can tell those two refusals apart without a
    /// file-identity check, and rhei does not take a dependency for one. #95's
    /// sidecar lock — held on a file the rename never touches — removes the
    /// window, and with it this hole.
    ///
    /// What is closed here: once *this* process has released its own lock the
    /// handle is never consulted again ([`read_through_handle`] answers `None`
    /// and the caller reports the original refusal), so the window this
    /// process opens cannot be read through by this process.
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
        read_through_handle(&self.file)
            .unwrap_or(Err(err))
            .map_err(|err| file_io_report(&self.path, action, err))
    }

    /// Release the lock and close the handle. Idempotent, and a no-op once a
    /// refused replace has already done it.
    fn release(&self) {
        if let Some(file) = self.file.lock().expect("plan lock handle").take() {
            let _ = fs2::FileExt::unlock(&file);
        }
        held_plan_locks()
            .lock()
            .expect("held plan locks")
            .retain(|(_, handle)| !Arc::ptr_eq(handle, &self.file));
    }
}

impl Drop for LockedPlanFile {
    fn drop(&mut self) {
        self.release();
    }
}

/// Every plan file this process currently holds locked.
///
/// On Windows a byte-range lock belongs to the handle that took it, so a second
/// open of the same file — from this very process — is refused. A command that
/// locks a plan and then hands the *path* to something that reads it therefore
/// reads nothing: `rhei new` locks the plan and then asks the loader to
/// validate it, and the loader knows about paths, not about locks. This is
/// where such a reader finds the handle that already holds the file.
// §FS-rhei-new.4
fn held_plan_locks() -> &'static Mutex<Vec<(PathBuf, PlanLockHandle)>> {
    static HELD_PLAN_LOCKS: OnceLock<Mutex<Vec<(PathBuf, PlanLockHandle)>>> = OnceLock::new();
    HELD_PLAN_LOCKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// The open, locked file, shared between the lock object and the registry.
type PlanLockHandle = Arc<Mutex<Option<fs::File>>>;

/// Read the whole file behind a lock handle, from the start.
///
/// `None` when the lock has already been released, which leaves the caller with
/// the original refusal to report. That is a rule and not an accident: a
/// released handle names whatever file it named before, and after
/// [`persist_locked`]'s rename that is an orphan nobody can reach by path. A
/// read served from it would be content no writer will ever see again.
fn read_through_handle(handle: &Mutex<Option<fs::File>>) -> Option<std::io::Result<String>> {
    let guard = handle.lock().expect("plan lock handle");
    // `None` once released — the caller keeps its original error. §FS-rhei-new.4
    let file = guard.as_ref()?;
    let mut reader = file;
    Some((|| {
        reader.seek(std::io::SeekFrom::Start(0))?;
        let mut raw = String::new();
        reader.read_to_string(&mut raw)?;
        Ok(raw)
    })())
}

/// Read `path` through whichever lock this process holds on it, if any.
fn read_through_held_lock(path: &Path) -> Option<std::io::Result<String>> {
    let held = {
        let locks = held_plan_locks().lock().expect("held plan locks");
        locks
            .iter()
            .find(|(locked_path, _)| same_path(locked_path, path))
            .map(|(_, handle)| handle.clone())
    }?;
    read_through_handle(&held)
}

/// Read `path`, by path first and through this process's own lock when the path
/// read is the one that lock refuses.
///
/// The ordering is [`LockedPlanFile::read_to_string`]'s, for its reasons: a
/// writer that took the lock before us has left a different file at `path`, and
/// only reading by path sees it. The fallback covers the case that reading by
/// path cannot: the file is one *we* locked, and Windows will not open it twice.
/// Its limit is that function's too — a lock we have released is deregistered
/// and never read through, and the window that remains is #95's.
///
/// The `io::Error` is passed through rather than wrapped, because the loader
/// this is installed into branches on its kind.
// §FS-rhei-new.4
fn plan_source_reader(path: &Path) -> std::io::Result<String> {
    let err = match fs::read_to_string(path) {
        Ok(raw) => return Ok(raw),
        Err(err) => err,
    };
    if !lock_is_contended(&err) {
        return Err(err);
    }
    read_through_held_lock(path).unwrap_or(Err(err))
}

/// [`plan_source_reader`] with the diagnostic a CLI caller wants; `action` names
/// the read the way `file_io_report` does.
// §FS-rhei-new.4
fn read_plan_source(path: &Path, action: &str) -> MietteResult<String> {
    plan_source_reader(path).map_err(|err| file_io_report(path, action, err))
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
