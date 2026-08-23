// The two things every `fs2` caller in this crate has to get right on more than
// one platform: reading a file you already hold the lock on, and telling
// "somebody else holds it" apart from "the probe itself failed".
//
// Its own part because both answers are platform facts rather than command
// behavior, and four call sites — the run-lock liveness probe, the headless
// launch lock, snapshot-continue, and every plan rewrite — got them wrong
// independently.

// §AR-source-file-size.3

/// Read the file at `path` while this process holds `handle`'s lock on it.
///
/// By path first, and that ordering is the point: every plan rewrite in rhei
/// replaces the file by renaming a temp file over it, so a writer that took the
/// lock before us has left a *different* file at `path` and `handle` still
/// names the one it replaced. Reading by path is what makes the lock protect
/// content rather than merely serialize.
///
/// The fallback is for Windows, where `fs2`'s locks are mandatory byte-range
/// locks rather than advisory ones: a second handle onto a file *this* process
/// has locked is refused outright, so the writer locks the plan and then cannot
/// read it. That refusal is the one case where `handle` is known to still be
/// the file at `path` — nobody replaced it, or the replacement would be
/// unlocked and the read would have succeeded — so reading through it is both
/// necessary and correct.
///
/// `action` names the read the way `file_io_report` wants it, so a caller's
/// diagnostic reads the same as it did when this was `fs::read_to_string`.
fn read_locked_to_string(handle: &fs::File, path: &Path, action: &str) -> MietteResult<String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(err) if lock_is_contended(&err) => {
            let mut reader = handle;
            reader
                .seek(std::io::SeekFrom::Start(0))
                .map_err(|err| file_io_report(path, action, err))?;
            let mut raw = String::new();
            reader.read_to_string(&mut raw).map_err(|err| file_io_report(path, action, err))?;
            Ok(raw)
        }
        Err(err) => Err(file_io_report(path, action, err)),
    }
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
