// How rhei finds the transcript an agent wrote for itself, and what it reads
// back out of the head of it: the optional `FlatById` locator keys
// (§FS-rhei-snapshots.9.1.1) and the observed target (§FS-rhei-snapshots.10.2.1).

// Split from snapshot_records.rs, which owns manifests and generation IO
// rather than transcript lookup. §AR-source-file-size.3

/// How the session id is recovered from a matched file's stem.
///
/// The session id is what a `ResumeStrategy` hands back to the agent, so it has
/// to be the agent's own id rather than whatever it named the file.
/// §FS-rhei-snapshots.9.1.1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StemSessionId {
    /// The whole file stem is the session id. The default, and v1 behavior.
    #[default]
    Whole,
    /// The last RFC-4122-shaped `8-4-4-4-12` hex run in the stem.
    TrailingUuid,
}

/// The `confirm_cwd_path` key path, paired with the working directory this
/// spawn's own transcript has to name. §FS-rhei-snapshots.9.1.1
#[derive(Clone, Debug)]
struct SnapshotCwdConfirmation {
    key_path: Vec<String>,
    /// Canonicalized, so the comparison is against the cwd the child process
    /// itself observes.
    spawn_working_dir: PathBuf,
}

/// The three optional `FlatById` locator keys, resolved at spawn against this
/// invocation's working directory. The default is the v1 behavior — a flat
/// `read_dir`, the whole stem as the id, and no confirmation — so a layout that
/// declares none of them keeps reading `<dir>/<id>.<ext>` exactly as it did.
/// §FS-rhei-snapshots.9.1.1 §FS-rhei-snapshots.10.1
#[derive(Clone, Debug, Default)]
struct SnapshotSessionLocator {
    nested: bool,
    id_from_stem: StemSessionId,
    confirm_cwd: Option<SnapshotCwdConfirmation>,
}

/// Read the locator keys off a `FlatById` layout.
///
/// A key that is present but malformed fails rather than falling back to the
/// default: reading `id_from_stem: "trailng_uuid"` as `Whole` would record a
/// session id that does not resume, which fails at the next spawn instead of
/// this one.
// §FS-rhei-snapshots.9.1.1
fn resolve_snapshot_session_locator(
    layout: &serde_json::Value,
    spawn_working_dir: &Path,
) -> MietteResult<SnapshotSessionLocator> {
    let nested = match layout.get("nested") {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(nested)) => *nested,
        Some(_) => {
            return Err(miette!(
                help = snapshot_help(),
                "session layout key 'nested' must be a boolean"
            ))
        }
    };
    // In settings the values are spelled `whole` and `trailing_uuid`, with the
    // Rust variant names accepted too — the tolerance `kind` already extends.
    let id_from_stem = match layout.get("id_from_stem") {
        None | Some(serde_json::Value::Null) => StemSessionId::Whole,
        Some(serde_json::Value::String(value)) => match value.as_str() {
            "whole" | "Whole" => StemSessionId::Whole,
            "trailing_uuid" | "trailing-uuid" | "TrailingUuid" => StemSessionId::TrailingUuid,
            other => {
                return Err(miette!(
                    help = snapshot_help(),
                    "session layout key 'id_from_stem' is '{}'; expected 'whole' or 'trailing_uuid'",
                    other
                ))
            }
        },
        Some(_) => {
            return Err(miette!(
                help = snapshot_help(),
                "session layout key 'id_from_stem' must be 'whole' or 'trailing_uuid'"
            ))
        }
    };
    let confirm_cwd = match layout.get("confirm_cwd_path") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Array(items)) if !items.is_empty() => {
            let mut key_path = Vec::with_capacity(items.len());
            for item in items {
                let Some(key) = item.as_str() else {
                    return Err(miette!(
                        help = snapshot_help(),
                        "session layout key 'confirm_cwd_path' must contain strings"
                    ));
                };
                key_path.push(key.to_string());
            }
            let canonical =
                rhei_core::platform::canonical_path(spawn_working_dir).map_err(|err| {
                    file_io_report(
                        spawn_working_dir,
                        "failed to canonicalize spawn working directory",
                        err,
                    )
                })?;
            Some(SnapshotCwdConfirmation { key_path, spawn_working_dir: canonical })
        }
        Some(_) => {
            return Err(miette!(
                help = snapshot_help(),
                "session layout key 'confirm_cwd_path' must be a non-empty array of strings"
            ))
        }
    };
    Ok(SnapshotSessionLocator { nested, id_from_stem, confirm_cwd })
}

/// The newest transcript under `dir` this invocation may claim, with the
/// session id read out of its stem.
///
/// Candidates are ranked newest-first and each is checked before it is taken,
/// so one that cannot be confirmed falls through to the next-newest rather than
/// being accepted or ending the scan.
// §FS-rhei-snapshots.9.1.1
fn newest_snapshot_session_file(
    dir: &Path,
    ext: &str,
    not_before: Option<std::time::SystemTime>,
    locator: &SnapshotSessionLocator,
) -> Option<(PathBuf, String)> {
    let mut candidates = Vec::new();
    collect_snapshot_session_candidates(dir, ext, not_before, locator.nested, &mut candidates);
    candidates.sort_by(|(left, _), (right, _)| right.cmp(left));
    candidates.into_iter().find_map(|(_, path)| {
        let stem = path.file_stem().and_then(OsStr::to_str)?;
        let session_id = snapshot_session_id_from_stem(stem, locator.id_from_stem)?;
        let confirmed = locator
            .confirm_cwd
            .as_ref()
            .is_none_or(|confirmation| snapshot_candidate_names_spawn_cwd(&path, confirmation));
        confirmed.then_some((path, session_id))
    })
}

fn collect_snapshot_session_candidates(
    dir: &Path,
    ext: &str,
    not_before: Option<std::time::SystemTime>,
    nested: bool,
    out: &mut Vec<(std::time::SystemTime, PathBuf)>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if nested {
            // The walk descends, and never through a symlink, so a link
            // pointing back up the tree cannot make it unbounded.
            // §FS-rhei-snapshots.9.1.1
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                collect_snapshot_session_candidates(&path, ext, not_before, nested, out);
                continue;
            }
        }
        if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some(ext) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        // A leftover transcript predates this invocation and is not its session.
        if not_before.is_some_and(|floor| modified < floor) {
            continue;
        }
        out.push((modified, path));
    }
}

/// The length of an RFC-4122 `8-4-4-4-12` rendering.
const UUID_TEXT_LEN: usize = 36;

// §FS-rhei-snapshots.9.1.1: a stem carrying no uuid is not a candidate, because
// an id that does not resume is worse than no snapshot.
fn snapshot_session_id_from_stem(stem: &str, rule: StemSessionId) -> Option<String> {
    match rule {
        StemSessionId::Whole => (!stem.is_empty()).then(|| stem.to_string()),
        StemSessionId::TrailingUuid => stem
            .as_bytes()
            .windows(UUID_TEXT_LEN)
            .rev()
            .find(|window| bytes_are_uuid_shaped(window))
            .and_then(|window| std::str::from_utf8(window).ok())
            .map(str::to_string),
    }
}

fn bytes_are_uuid_shaped(bytes: &[u8]) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut index = 0;
    for (position, group) in GROUPS.iter().enumerate() {
        if position > 0 {
            if bytes.get(index) != Some(&b'-') {
                return false;
            }
            index += 1;
        }
        for _ in 0..*group {
            match bytes.get(index) {
                Some(byte) if byte.is_ascii_hexdigit() => index += 1,
                _ => return false,
            }
        }
    }
    index == bytes.len()
}

/// Whether a candidate's own header names this spawn's working directory.
///
/// A fixed directory an agent derives for itself is shared by every run on the
/// machine, so anything that stops the header being read — missing, unreadable,
/// unparsable, or carrying nothing at the key path — is a rejection. Capturing
/// a transcript that belongs to someone else's session is worse than emitting
/// none, because the snapshot is wrong rather than absent.
// §FS-rhei-snapshots.9.1.1
fn snapshot_candidate_names_spawn_cwd(
    path: &Path,
    confirmation: &SnapshotCwdConfirmation,
) -> bool {
    let Some(record) = first_json_record(path) else {
        return false;
    };
    let mut cursor = &record;
    for key in &confirmation.key_path {
        let Some(next) = cursor.get(key) else {
            return false;
        };
        cursor = next;
    }
    let Some(recorded) = cursor.as_str().filter(|text| !text.trim().is_empty()) else {
        return false;
    };
    // Both sides canonicalized; a header naming a directory that is not on this
    // machine cannot be canonicalized and simply does not match.
    let recorded = Path::new(recorded);
    let canonical = rhei_core::platform::canonical_path(recorded);
    canonical.as_deref().unwrap_or(recorded) == confirmation.spawn_working_dir.as_path()
}

/// The transcript's first non-empty record, parsed. `None` when the file cannot
/// be opened, holds nothing, or does not open with JSON.
// §FS-rhei-snapshots.9.1.1
fn first_json_record(path: &Path) -> Option<serde_json::Value> {
    let file = fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return serde_json::from_str(trimmed).ok();
    }
}

/// The transcript emit reads: the redirected `session_dir`, else the fixed
/// `dir_template` dir at an exact assigned-id path, else a scan through the
/// locator this spawn resolved.
// §FS-rhei-snapshots.10.2 §FS-rhei-snapshots.9.1.1
fn transcript_source_for_snapshot(
    preload: &SnapshotPreload,
    layout: &serde_json::Value,
) -> Option<(PathBuf, String, String)> {
    let ext = snapshot_layout_ext(layout)?;
    if snapshot_layout_kind(layout).as_deref() != Some("FlatById") {
        return None;
    }
    if let Some(session_dir) = preload.session_dir.as_deref() {
        // A directory rhei redirected the transcript into has nothing to search
        // for, so the locator keys do not govern it. §FS-rhei-snapshots.9.1.1
        let (path, session_id) = newest_snapshot_session_file(
            session_dir,
            &ext,
            None,
            &SnapshotSessionLocator::default(),
        )?;
        return Some((path, ext, session_id));
    }
    let fixed_dir = preload.fixed_session_dir.as_deref()?;
    if let Some(id) = preload.fixed_session_id.as_deref() {
        let path = fixed_dir.join(format!("{id}.{ext}"));
        return path.is_file().then_some((path, ext, id.to_string()));
    }
    let (path, session_id) = newest_snapshot_session_file(
        fixed_dir,
        &ext,
        preload.fixed_session_scan_floor,
        &preload.fixed_session_locator,
    )?;
    Some((path, ext, session_id))
}

/// How far into a `jsonl` transcript the observed-target read looks.
///
/// Four times the deepest position observed in practice, and still a bounded
/// read of the head of one file rather than a scan of a transcript that may be
/// tens of megabytes. §FS-rhei-snapshots.10.2.1
const SNAPSHOT_HEADER_SCAN_LINES: usize = 32;

/// Provider key paths, tried in order within each record.
// §FS-rhei-snapshots.10.2.1
const OBSERVED_PROVIDER_PATHS: &[&[&str]] = &[
    &["provider"],
    &["model", "provider"],
    &["target", "provider"],
    &["session", "provider"],
    &["payload", "model_provider"],
];

/// Model key paths, tried in order within each record.
// §FS-rhei-snapshots.10.2.1
const OBSERVED_MODEL_PATHS: &[&[&str]] = &[
    &["model"],
    &["model_name"],
    &["model", "name"],
    &["model", "model"],
    &["target", "model"],
    &["session", "model"],
    &["payload", "model"],
];

/// The provider and the model the head of a `jsonl` transcript records, each
/// taken independently of the other.
///
/// One agent writes both in a single session header; another writes the
/// provider in its session header and the model only when the first turn opens,
/// so requiring one record to carry the pair would find neither. Records
/// carrying neither are skipped rather than ending the scan.
// §FS-rhei-snapshots.10.2.1
fn jsonl_observed_target(transcript_source: &Path) -> (Option<String>, Option<String>) {
    let Ok(file) = fs::File::open(transcript_source) else {
        return (None, None);
    };
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut provider = None;
    let mut model = None;
    for _ in 0..SNAPSHOT_HEADER_SCAN_LINES {
        if provider.is_some() && model.is_some() {
            break;
        }
        line.clear();
        if !matches!(reader.read_line(&mut line), Ok(read) if read > 0) {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        provider = provider.or_else(|| snapshot_header_string(&value, OBSERVED_PROVIDER_PATHS));
        model = model.or_else(|| snapshot_header_string(&value, OBSERVED_MODEL_PATHS));
    }
    (provider, model)
}

fn snapshot_header_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut cursor = value;
        for key in *path {
            cursor = cursor.get(*key)?;
        }
        cursor.as_str().filter(|text| !text.trim().is_empty()).map(str::to_string)
    })
}

/// `observed_provider` and `observed_model` for a manifest.
///
/// One rule for every agent: "which agent is this" is not a question the emit
/// path is allowed to ask — a profile declares a layout, and rhei reads what
/// that layout produced. Each field the window does not yield falls back to its
/// declared counterpart, and the run warns naming the transcript; the snapshot
/// is still written, because an observed target that is merely the declared one
/// is a weaker `cache_beneficial` signal, not a failed emit.
// §FS-rhei-snapshots.10.2.1
fn observed_snapshot_target(
    resolved: &ResolvedAgent,
    transcript_source: &Path,
    transcript_ext: &str,
) -> (String, String) {
    let declared_provider = snapshot_declared_provider(resolved).to_string();
    let declared_model = snapshot_declared_model(resolved).to_string();
    if transcript_ext != "jsonl" {
        return (declared_provider, declared_model);
    }
    let (provider, model) = jsonl_observed_target(transcript_source);
    let missing = match (provider.is_none(), model.is_none()) {
        (true, true) => Some("provider or model"),
        (true, false) => Some("provider"),
        (false, true) => Some("model"),
        (false, false) => None,
    };
    if let Some(missing) = missing {
        diag_warn!(
            "warning: snapshot transcript '{}' records no {} in its first {} lines; falling back to the declared target {}:{}",
            transcript_source.display(),
            missing,
            SNAPSHOT_HEADER_SCAN_LINES,
            declared_provider,
            declared_model
        );
    }
    (provider.unwrap_or(declared_provider), model.unwrap_or(declared_model))
}
