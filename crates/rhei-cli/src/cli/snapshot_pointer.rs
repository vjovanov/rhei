// The `current` pointer of a snapshot identity: the one file every generation
// writer updates and every lineage reader follows.
//
// Its own part because the pointer has two spellings — a relative symlink where
// the platform grants them, a one-line file where it does not — and reader and
// writer have to agree on both. They had disagreed: the writers produced either
// spelling and the reader knew only one, so on Windows every cached generation
// read as stale.

// §AR-source-file-size.3 §FS-rhei-snapshots.7

/// The generation an identity's `current` pointer names, in either spelling the
/// writer may have used.
///
/// Both are tried on every platform rather than the local one only: a snapshot
/// cache is a directory that travels between machines, so the platform reading
/// a pointer is not necessarily the one that wrote it.
// §FS-rhei-snapshots.7
fn snapshot_current_target(identity_dir: &Path) -> Option<PathBuf> {
    let current = identity_dir.join("current");
    if let Ok(target) = fs::read_link(&current) {
        return Some(target);
    }
    let named = fs::read_to_string(&current).ok()?;
    let named = named.trim();
    (!named.is_empty()).then(|| PathBuf::from(named))
}

/// Point `current` at `target`, a generation directory name such as `g3`.
///
/// `nonce` distinguishes this writer's temp pointer from a concurrent one; the
/// caller passes the same nonce it named the generation's own temp directory
/// with, so an interrupted write leaves debris that is traceable to one attempt.
// §FS-rhei-snapshots.7 §FS-rhei-snapshots.7.2
fn replace_current_pointer(identity_dir: &Path, target: &str, nonce: &str) -> MietteResult<()> {
    #[cfg(unix)]
    {
        write_current_pointer_symlink(identity_dir, target, nonce)
    }
    #[cfg(not(unix))]
    {
        write_current_pointer_file(identity_dir, target, nonce)
    }
}

/// The symlink spelling: `current` becomes a relative link to `target`.
#[cfg(unix)]
// §FS-rhei-snapshots.7
fn write_current_pointer_symlink(
    identity_dir: &Path,
    target: &str,
    nonce: &str,
) -> MietteResult<()> {
    use std::os::unix::fs::symlink;
    let tmp = current_pointer_tmp(identity_dir, nonce);
    remove_stale_pointer_tmp(&tmp)?;
    symlink(target, &tmp)
        .map_err(|err| file_io_report(&tmp, "failed to write current tmp pointer", err))?;
    rename_pointer_into_place(&tmp, identity_dir)
}

/// The regular-file spelling: `current` becomes a one-line file naming
/// `target`, for a platform that grants no unprivileged symlinks.
///
/// Written through the same temp-and-rename as the symlink spelling rather than
/// in place. Writing in place is not a smaller version of this — it is a
/// different guarantee: a reader can observe the truncated file, and a write
/// that dies leaves no pointer at all where the previous generation would still
/// have been current. `rename` over an existing file is atomic on Windows too
/// (`MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`, which is what `fs::rename`
/// asks for there).
// §FS-rhei-snapshots.7 §FS-rhei-snapshots.7.2
#[cfg(any(not(unix), test))]
fn write_current_pointer_file(identity_dir: &Path, target: &str, nonce: &str) -> MietteResult<()> {
    let tmp = current_pointer_tmp(identity_dir, nonce);
    remove_stale_pointer_tmp(&tmp)?;
    fs::write(&tmp, format!("{target}\n"))
        .map_err(|err| file_io_report(&tmp, "failed to write current tmp pointer", err))?;
    rename_pointer_into_place(&tmp, identity_dir)
}

fn current_pointer_tmp(identity_dir: &Path, nonce: &str) -> PathBuf {
    identity_dir.join(format!("current.tmp-{nonce}"))
}

/// Clear a temp pointer left by an interrupted write with this same nonce, so
/// the write that follows is a create rather than a refusal.
fn remove_stale_pointer_tmp(tmp: &Path) -> MietteResult<()> {
    if !tmp.exists() && !tmp.is_symlink() {
        return Ok(());
    }
    fs::remove_file(tmp)
        .map_err(|err| file_io_report(tmp, "failed to remove stale current tmp pointer", err))
}

fn rename_pointer_into_place(tmp: &Path, identity_dir: &Path) -> MietteResult<()> {
    let current = identity_dir.join("current");
    fs::rename(tmp, &current)
        .map_err(|err| file_io_report(&current, "failed to update current pointer", err))
}
