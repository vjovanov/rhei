// Linux-only proof that a recorded process owns the run-lock inode after its
// pathname has been renamed, unlinked, or replaced.
// §FS-rhei-run-headless.3

#[cfg(target_os = "linux")]
enum ProcessProbe {
    OwnsLock,
    DoesNotOwnLock,
    Gone,
    Unknown(String),
}

#[cfg(target_os = "linux")]
#[derive(serde::Serialize, serde::Deserialize)]
struct RunLockOwner {
    version: u8,
    id: String,
    pid: u32,
    workspace: PathBuf,
    process_start_ticks: u64,
}

#[cfg(target_os = "linux")]
const RUN_LOCK_OWNER_MAX_BYTES: u64 = 16 * 1024;

/// Record enough identity on the held inode for a later `/proc` inspection to
/// distinguish this process from a reuse of its numeric pid.
// §FS-rhei-run-headless.3
#[cfg(target_os = "linux")]
fn write_run_lock_owner(lock: &mut HeldRunLock, id: &str, pid: u32) -> Result<(), String> {
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = fs::read_to_string(&stat_path)
        .map_err(|err| format!("{} could not be read: {err}", stat_path.display()))?;
    let process_start_ticks = parse_linux_process_start_ticks(&stat)?;
    let owner = RunLockOwner {
        version: 1,
        id: id.to_string(),
        pid,
        workspace: lock.workspace.clone(),
        process_start_ticks,
    };
    let mut body = serde_json::to_vec(&owner)
        .map_err(|err| format!("run-lock ownership could not be serialized: {err}"))?;
    body.push(b'\n');
    lock.file.rewind().map_err(|err| format!("run lock could not be rewound: {err}"))?;
    lock.file.set_len(0).map_err(|err| format!("run lock could not be cleared: {err}"))?;
    lock.file
        .write_all(&body)
        .and_then(|()| lock.file.flush())
        .map_err(|err| format!("run-lock ownership could not be written: {err}"))
}

/// Prove that the descriptor's exact Linux process identity owns a locked file
/// descriptor carrying this run's lock record. Merely finding the pid is not
/// evidence: it may now belong to an unrelated process.
// §FS-rhei-run-headless.3
#[cfg(target_os = "linux")]
fn probe_recorded_lock_owner(descriptor: &RunDescriptor) -> ProcessProbe {
    if descriptor.pid == 0 {
        return ProcessProbe::Unknown("recorded pid 0 does not identify one process".to_string());
    }
    let proc_root = PathBuf::from(format!("/proc/{}", descriptor.pid));
    let stat_path = proc_root.join("stat");
    let stat = match fs::read_to_string(&stat_path) {
        Ok(stat) => stat,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ProcessProbe::Gone,
        Err(err) => {
            return ProcessProbe::Unknown(format!(
                "{} could not be read: {err}",
                stat_path.display()
            ));
        }
    };
    let process_start_ticks = match parse_linux_process_start_ticks(&stat) {
        Ok(start) => start,
        Err(reason) => return ProcessProbe::Unknown(reason),
    };
    let fdinfo_dir = proc_root.join("fdinfo");
    let entries = match fs::read_dir(&fdinfo_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ProcessProbe::Gone,
        Err(err) => {
            return ProcessProbe::Unknown(format!(
                "{} could not be read: {err}",
                fdinfo_dir.display()
            ));
        }
    };

    let lock_path = descriptor.workspace.join(".rhei/run.lock");
    let mut inconclusive = None;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                inconclusive = Some(format!("{} could not be inspected: {err}", fdinfo_dir.display()));
                continue;
            }
        };
        let fd = entry.file_name();
        if fd.to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let fdinfo = match fs::read_to_string(entry.path()) {
            Ok(fdinfo) => fdinfo,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be read: {err}", entry.path().display()));
                continue;
            }
        };
        if !fdinfo_holds_exclusive_flock(&fdinfo, descriptor.pid) {
            continue;
        }

        let fd_path = proc_root.join("fd").join(&fd);
        let target = match fs::read_link(&fd_path) {
            Ok(target) => target,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be inspected: {err}", fd_path.display()));
                continue;
            }
        };
        let metadata = match fs::metadata(&fd_path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be inspected: {err}", fd_path.display()));
                continue;
            }
        };
        if !metadata.is_file() {
            continue;
        }
        let file = match fs::File::open(&fd_path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                inconclusive = Some(format!("{} could not be read: {err}", fd_path.display()));
                continue;
            }
        };
        let mut body = Vec::new();
        if let Err(err) = file.take(RUN_LOCK_OWNER_MAX_BYTES + 1).read_to_end(&mut body) {
            inconclusive = Some(format!("{} could not be read: {err}", fd_path.display()));
            continue;
        }
        if body.len() as u64 > RUN_LOCK_OWNER_MAX_BYTES {
            if displaced_run_lock_target(&target, &lock_path) {
                inconclusive = Some(format!(
                    "{} is locked by pid {} but its ownership record is too large",
                    target.display(),
                    descriptor.pid
                ));
            }
            continue;
        }
        let owner = match serde_json::from_slice::<RunLockOwner>(&body) {
            Ok(owner) => owner,
            Err(_) if displaced_run_lock_target(&target, &lock_path) => {
                inconclusive = Some(format!(
                    "{} is locked by pid {} but carries no verifiable run ownership",
                    target.display(),
                    descriptor.pid
                ));
                continue;
            }
            Err(_) => continue,
        };
        if owner.version == 1
            && owner.id == descriptor.id
            && owner.pid == descriptor.pid
            && owner.workspace == descriptor.workspace
            && owner.process_start_ticks == process_start_ticks
        {
            return ProcessProbe::OwnsLock;
        }
    }
    inconclusive.map_or(ProcessProbe::DoesNotOwnLock, ProcessProbe::Unknown)
}

#[cfg(target_os = "linux")]
fn parse_linux_process_start_ticks(stat: &str) -> Result<u64, String> {
    let fields = stat
        .rsplit_once(") ")
        .map(|(_, fields)| fields)
        .ok_or_else(|| "the recorded process stat has no command terminator".to_string())?;
    fields
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| "the recorded process stat has no start identity".to_string())?
        .parse::<u64>()
        .map_err(|err| format!("the recorded process start identity is invalid: {err}"))
}

#[cfg(target_os = "linux")]
fn fdinfo_holds_exclusive_flock(fdinfo: &str, pid: u32) -> bool {
    let pid = pid.to_string();
    fdinfo.lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.len() >= 6
            && fields[0] == "lock:"
            && fields[2] == "FLOCK"
            && fields[3] == "ADVISORY"
            && fields[4] == "WRITE"
            && fields[5] == pid
    })
}

#[cfg(target_os = "linux")]
fn displaced_run_lock_target(target: &Path, lock_path: &Path) -> bool {
    let target_text = target.to_string_lossy();
    let lock_text = lock_path.to_string_lossy();
    if target_text == lock_text || target_text == format!("{lock_text} (deleted)") {
        return true;
    }
    target.parent() == lock_path.parent()
        && target
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with("run.lock."))
}
