// The lock a create holds, and the write it makes under it.
//
// Its own part because serializing a create is a concurrency concern that knows
// nothing about ids, markdown, or state machines: it only knows which file
// stands for the scope being written to.

// §FS-rhei-new.4

/// An exclusive advisory lock held for the length of one create.
///
/// Held from before the first plan load through the write, both validation
/// passes, and any rollback. Anything narrower still loses tickets: the sibling
/// numbering is read from the same file the write then modifies, and a rollback
/// restores a snapshot taken before it. `Drop` releases it on every exit path,
/// and the OS releases it if the process dies — an advisory lock cannot go
/// stale, so there is nothing to reap.
// §FS-rhei-new.4
struct NewCreateLock {
    file: fs::File,
}

impl Drop for NewCreateLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

/// Take the create lock for `target`, blocking until it is free.
///
/// Blocking rather than failing, like every other plan-file lock rhei takes: a
/// create that gave up on a busy project would hand the caller back exactly the
/// race the lock exists to remove.
// §FS-rhei-new.4
fn lock_new_create(target: &Path) -> MietteResult<NewCreateLock> {
    let path = new_create_lock_path(target);
    let file = fs::File::open(&path)
        .map_err(|err| file_io_report(&path, "failed to open for locking", err))?;
    file.lock_exclusive()
        .map_err(|err| file_io_report(&path, "failed to acquire file lock", err))?;
    Ok(NewCreateLock { file })
}

/// The file that stands for the whole create scope: the project manifest for a
/// project, the index for a bare workspace, the plan file itself for a lone
/// plan. Each already exists wherever a create is legal, so locking adds no
/// artifact to the tree and nothing new to ignore.
// §FS-rhei-new.4
fn new_create_lock_path(target: &Path) -> PathBuf {
    if let Some(project_dir) = workspace::panta_project_dir(target) {
        return project_dir.join(workspace::PANTA_INDEX_FILE);
    }
    if let Some(workspace_dir) = workspace::workspace_dir(target) {
        return workspace_dir.join("index.rhei.md");
    }
    target.to_path_buf()
}

/// Write a plan file, replacing an existing one through a temp file in its own
/// directory so an interrupted create cannot leave a truncated plan where a
/// whole one was.
///
/// A file that does not exist yet is written directly: there is no previous
/// content a partial write could destroy, a failed create removes it whole
/// anyway, and a plain write gives it the mode a plain create would.
// §FS-rhei-new.4 §FS-rhei-new.5.1
fn write_plan_file_atomically(path: &Path, contents: &str) -> MietteResult<()> {
    let Ok(existing) = fs::metadata(path) else {
        return fs::write(path, contents)
            .map_err(|err| file_io_report(path, "failed to write", err));
    };
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|err| {
        miette!(help = temp_write_help(), "failed to create temp file next to {}: {err}",
            display_path(path))
    })?;
    tmp.write_all(contents.as_bytes())
        .map_err(|err| miette!(help = temp_write_help(), "failed to write temp file: {err}"))?;
    // The replacement is the same file to its author, so it keeps the file's
    // own permissions rather than a temp file's private ones.
    let _ = tmp.as_file().set_permissions(existing.permissions());
    tmp.persist(path).map_err(|err| {
        miette!(help = temp_write_help(), "failed to persist {}: {err}", display_path(path))
    })?;
    Ok(())
}
