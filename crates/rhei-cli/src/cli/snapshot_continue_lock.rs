// §FS-rhei-snapshot-operations.1.5: Continue interactively from a cached snapshot.
fn snapshot_continue_command(
    ctx: &SnapshotCommandContext,
    reference: &str,
    target: Option<&str>,
    generation: Option<u64>,
    no_capture: bool,
) -> MietteResult<()> {
    let Some(_run_lock) = try_acquire_run_lock(&ctx.workspace_root)? else {
        return Err(miette!(
            "rhei snapshot continue cannot run while .rhei/run.lock is held; stop the run first"
        ));
    };
    let record = resolve_snapshot_ref(ctx, reference, target, generation)?;
    if record.completion == "timeout" {
        eprintln!(
            "warning: snapshot {} completed by timeout; the native transcript may be truncated",
            record.display_ref()
        );
    }
    let resolved = resolve_snapshot_continue_agent(ctx, &record)?;
    let Some(session) = resolved.profile.session.as_ref() else {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' does not expose a resume strategy, session layout, and interactive continuation profile",
            resolved.agent.id()
        ));
    };
    if !profile_supports_interactive_continue(&resolved.profile.session) {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' does not expose a resume strategy, session layout, and interactive continuation profile",
            resolved.agent.id()
        ));
    }
    if !no_capture && snapshot_session_string(session, "session_dir_flag").is_none() {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' cannot capture interactive continuation without session_dir_flag",
            resolved.agent.id()
        ));
    }
    // Continue preloads the native session identified by the source manifest.
    // Reject settings drift before staging or claiming parent lineage. §FS-rhei-snapshots
    if let Some(reason) = snapshot_record_native_incompatibility(&record, &resolved) {
        return Err(miette!(
            "incompatible-snapshot: selected snapshot {} is not native-compatible with agent '{}': {}",
            record.display_ref(),
            resolved.agent.id(),
            reason
        ));
    }

    let preload = prepare_snapshot_continue_preload(&ctx.workspace_root, &record, session)?;
    let status = spawn_snapshot_continue_agent(ctx, &record, &resolved, session, &preload.inner)?;
    let completion = if status.success() {
        SnapshotCompletion::Success
    } else {
        SnapshotCompletion::Failure
    };
    if !no_capture {
        let captured = capture_snapshot_continue_generation(
            ctx, &record, &resolved, session, &preload, completion,
        )?;
        println!("captured {}", captured.display_ref());
        println!(
            "hint: operator generations are hidden by the default list view; use `rhei snapshot list --produced-by operator` or `--produced-by all`."
        );
    } else if status.success() {
        println!(
            "continued from {} without capture; no snapshot written",
            record.display_ref()
        );
    }
    if status.success() {
        Ok(())
    } else {
        Err(miette!(
            "snapshot continue agent '{}' exited with status {}",
            resolved.agent.id(),
            status
        ))
    }
}

#[derive(Debug)]
struct SnapshotContinuePreload {
    inner: SnapshotPreload,
    staged_source: Option<SnapshotContinueStagedSource>,
}

#[derive(Debug)]
struct SnapshotContinueStagedSource {
    path: PathBuf,
    sha256: String,
    bytes: u64,
}

fn profile_supports_interactive_continue(session: &Option<serde_json::Value>) -> bool {
    let Some(session) = session.as_ref() else {
        return false;
    };
    let has_interactive = session.get("interactive").is_some_and(|interactive| {
        interactive.is_object()
            && interactive
                .get("args")
                .is_none_or(|args| args.as_array().is_some_and(|items| {
                    items.iter().all(serde_json::Value::is_string)
                }))
            && interactive
                .get("command")
                .is_none_or(|command| command.as_array().is_some_and(|items| {
                    !items.is_empty() && items.iter().all(serde_json::Value::is_string)
                }))
    });
    profile_has_snapshot_preload(&Some(session.clone())) && has_interactive
}

fn resolve_snapshot_continue_agent(
    ctx: &SnapshotCommandContext,
    record: &SnapshotRecord,
) -> MietteResult<ResolvedAgent> {
    let selector = snapshot_record_target_selector(record)?;
    let target = parse_execution_target(&selector)
        .map_err(|err| miette!("snapshot target selector '{}' is invalid: {}", selector, err))?;
    if !ctx.settings.agents.contains_key(target.agent.as_str()) {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' is not configured",
            target.agent
        ));
    }
    resolve_target_agent(&selector, None, &ctx.settings)
}

fn snapshot_record_target_selector(record: &SnapshotRecord) -> MietteResult<String> {
    record
        .manifest
        .get("target")
        .and_then(|target| target.get("selector"))
        .and_then(serde_json::Value::as_str)
        .filter(|selector| !selector.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            miette!(
                "invalid snapshot manifest for {}: missing target.selector",
                record.display_ref()
            )
        })
}

fn prepare_snapshot_continue_preload(
    workspace_root: &Path,
    record: &SnapshotRecord,
    session: &serde_json::Value,
) -> MietteResult<SnapshotContinuePreload> {
    let mut preload = SnapshotPreload::default();
    let mut staged_source = None;
    if let Some(flag) = snapshot_session_string(session, "session_dir_flag") {
        let dir = snapshot_session_dir(
            workspace_root,
            &record.task_id,
            &record.emitting_state,
            &record.target_slug,
        );
        fs::create_dir_all(&dir).map_err(|err| {
            file_io_report(&dir, "failed to create snapshot continue session dir", err)
        })?;
        preload.extra_args.push(flag);
        preload.extra_args.push(dir.display().to_string());
        preload.session_dir = Some(dir);
    }

    if let Some(flag) = snapshot_strategy_flag(session, "fork") {
        preload.extra_args.push(flag);
        preload.extra_args.push(record.transcript_path().display().to_string());
    } else if let Some(flag) = snapshot_strategy_flag(session, "resume") {
        let session_id = record
            .manifest
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        preload.extra_args.push(flag);
        preload.extra_args.push(session_id.to_string());
        if let Some(session_dir) = preload.session_dir.as_ref() {
            staged_source = Some(stage_snapshot_continue_resume_source(record, session_dir)?);
        }
    } else {
        return Err(miette!(
            "unsupported-snapshot-session: agent profile has no supported snapshot resume or fork strategy"
        ));
    }
    preload.parent_ref = Some(snapshot_parent_ref(record));
    Ok(SnapshotContinuePreload { inner: preload, staged_source })
}

fn stage_snapshot_continue_resume_source(
    record: &SnapshotRecord,
    session_dir: &Path,
) -> MietteResult<SnapshotContinueStagedSource> {
    let ext = record
        .manifest
        .get("session_layout")
        .and_then(snapshot_layout_ext)
        .unwrap_or_else(|| "jsonl".to_string());
    let session_id =
        record.manifest.get("session_id").and_then(serde_json::Value::as_str).unwrap_or("source");
    let target = session_dir.join(format!("{session_id}.{ext}"));
    fs::copy(record.transcript_path(), &target).map_err(|err| {
        file_io_report(&target, "failed to stage snapshot transcript for continue", err)
    })?;
    snapshot_continue_staged_source(&target)
}

fn snapshot_continue_staged_source(path: &Path) -> MietteResult<SnapshotContinueStagedSource> {
    let bytes = fs::read(path)
        .map_err(|err| file_io_report(path, "failed to read staged snapshot transcript", err))?;
    Ok(SnapshotContinueStagedSource {
        path: path.to_path_buf(),
        sha256: sha256_hex(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn spawn_snapshot_continue_agent(
    ctx: &SnapshotCommandContext,
    record: &SnapshotRecord,
    resolved: &ResolvedAgent,
    session: &serde_json::Value,
    preload: &SnapshotPreload,
) -> MietteResult<std::process::ExitStatus> {
    let command_parts = snapshot_interactive_command(resolved, session)?;
    let (program, base_args) = command_parts.split_first().ok_or_else(|| {
        miette!(
            "unsupported-snapshot-session: agent '{}' interactive command is empty",
            resolved.agent.id()
        )
    })?;
    let mut cmd = std::process::Command::new(program);
    cmd.current_dir(&ctx.workspace_root);
    for arg in base_args {
        cmd.arg(arg);
    }
    if let Some(mode) = resolved.mode.as_deref() {
        if let Some(flags) = resolved.profile.modes.get(mode) {
            for arg in flags {
                cmd.arg(arg);
            }
        }
    }
    if let (Some(flag), Some(model)) = (
        resolved.profile.model_flag.as_deref(),
        resolved.model_name.as_deref().or(resolved.model.as_deref()),
    ) {
        cmd.arg(flag).arg(model);
    }
    for arg in snapshot_interactive_args(session)? {
        cmd.arg(arg);
    }
    for arg in &preload.extra_args {
        cmd.arg(arg);
    }
    cmd.env("RHEI_PLAN_PATH", &ctx.plan_path)
        .env("RHEI_TASK_ID", &record.task_id)
        .env("RHEI_STATE", &record.emitting_state)
        .env("RHEI_AGENT", resolved.agent.id());
    if let Some(model) = &resolved.model {
        cmd.env("RHEI_MODEL", model);
    }
    if let Some(mode) = &resolved.mode {
        cmd.env("RHEI_AGENT_MODE", mode);
    }
    if let Some(target) = &resolved.target {
        cmd.env("RHEI_TARGET", target.selector());
        cmd.env("RHEI_TARGET_SLUG", target.slug());
    }
    if let Some(provider) = resolved.model_provider.as_deref() {
        cmd.env("RHEI_MODEL_PROVIDER", provider);
    }
    if let Some(model_name) = resolved.model_name.as_deref() {
        cmd.env("RHEI_MODEL_NAME", model_name);
    }
    if let Some(session_dir) = preload.session_dir.as_ref() {
        cmd.env("RHEI_SNAPSHOT_SESSION_DIR", session_dir);
    }
    if let Some(parent_ref) = preload.parent_ref.as_ref() {
        cmd.env("RHEI_SNAPSHOT_PARENT_REF", parent_ref.to_string());
    }
    cmd.stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let status = cmd
        .status()
        .map_err(|err| miette!("failed to spawn agent '{}': {err}", resolved.agent.id()))?;
    Ok(status)
}

fn snapshot_interactive_command(
    resolved: &ResolvedAgent,
    session: &serde_json::Value,
) -> MietteResult<Vec<String>> {
    let Some(interactive) = session.get("interactive") else {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' has no interactive continuation profile",
            resolved.agent.id()
        ));
    };
    match interactive.get("command") {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    miette!(
                        "unsupported-snapshot-session: interactive.command must contain strings"
                    )
                })
            })
            .collect(),
        Some(_) => Err(miette!(
            "unsupported-snapshot-session: interactive.command must be an array of strings"
        )),
        None => Ok(resolved.profile.command.clone()),
    }
}

fn snapshot_interactive_args(session: &serde_json::Value) -> MietteResult<Vec<String>> {
    let Some(interactive) = session.get("interactive") else {
        return Ok(Vec::new());
    };
    match interactive.get("args") {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    miette!("unsupported-snapshot-session: interactive.args must contain strings")
                })
            })
            .collect(),
        Some(_) => Err(miette!(
            "unsupported-snapshot-session: interactive.args must be an array of strings"
        )),
        None => Ok(Vec::new()),
    }
}

// §FS-rhei-snapshots.8: Operator generations use the normal manifest schema.
fn capture_snapshot_continue_generation(
    ctx: &SnapshotCommandContext,
    source: &SnapshotRecord,
    resolved: &ResolvedAgent,
    session: &serde_json::Value,
    preload: &SnapshotContinuePreload,
    completion: SnapshotCompletion,
) -> MietteResult<SnapshotRecord> {
    let layout = snapshot_session_layout(session).ok_or_else(|| {
        miette!(
            "unsupported-snapshot-session: agent '{}' has no supported snapshot session layout",
            resolved.agent.id()
        )
    })?;
    let Some(session_layout) = snapshot_layout_manifest(session) else {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' has an incomplete snapshot session layout",
            resolved.agent.id()
        ));
    };
    let Some((transcript_source, transcript_ext, session_id)) =
        transcript_source_for_snapshot_continue(
            preload.inner.session_dir.as_deref(),
            layout,
            preload.staged_source.as_ref(),
        )?
    else {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' did not produce a supported native session transcript",
            resolved.agent.id()
        ));
    };
    let target_selector = snapshot_record_target_selector(source)?;
    let parent_ref = snapshot_parent_ref(source);
    let (observed_provider, observed_model) =
        observed_snapshot_target(resolved, &transcript_source, &transcript_ext);
    write_snapshot_generation_atomic(
        &ctx.cache_root,
        &ctx.workspace_root,
        &ctx.settings,
        &source.task_id,
        &source.snapshot_name,
        &source.emitting_state,
        source.visit,
        &source.target_slug,
        &target_selector,
        resolved,
        session_layout,
        &session_id,
        &transcript_source,
        &transcript_ext,
        &observed_provider,
        &observed_model,
        Some(&parent_ref),
        completion,
        SnapshotProducedBy::Operator,
        None,
    )
}

fn transcript_source_for_snapshot_continue(
    session_dir: Option<&Path>,
    layout: &serde_json::Value,
    staged_source: Option<&SnapshotContinueStagedSource>,
) -> MietteResult<Option<(PathBuf, String, String)>> {
    let Some(session_dir) = session_dir else {
        return Ok(None);
    };
    let Some(ext) = snapshot_layout_ext(layout) else {
        return Ok(None);
    };
    if snapshot_layout_kind(layout).as_deref() != Some("FlatById") {
        return Ok(None);
    }
    let entries = fs::read_dir(session_dir).map_err(|err| {
        file_io_report(session_dir, "failed to inspect snapshot continue session dir", err)
    })?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries {
        let entry = entry.map_err(|err| {
            file_io_report(session_dir, "failed to inspect snapshot continue session entry", err)
        })?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some(ext.as_str()) {
            continue;
        }
        if snapshot_continue_path_is_unchanged_staged_source(&path, staged_source)? {
            continue;
        }
        let modified = entry.metadata().and_then(|metadata| metadata.modified()).map_err(|err| {
            file_io_report(&path, "failed to inspect snapshot continue transcript", err)
        })?;
        if newest.as_ref().is_none_or(|(existing, _)| modified > *existing) {
            newest = Some((modified, path));
        }
    }
    let Some((_, path)) = newest else {
        return Ok(None);
    };
    let session_id = path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| {
            miette!("unsupported-snapshot-session: snapshot continue transcript has no session id")
        })?
        .to_string();
    Ok(Some((path, ext, session_id)))
}

fn snapshot_continue_path_is_unchanged_staged_source(
    path: &Path,
    staged_source: Option<&SnapshotContinueStagedSource>,
) -> MietteResult<bool> {
    let Some(staged_source) = staged_source else {
        return Ok(false);
    };
    if path != staged_source.path {
        return Ok(false);
    }
    let metadata = fs::metadata(path)
        .map_err(|err| file_io_report(path, "failed to inspect staged snapshot transcript", err))?;
    if metadata.len() != staged_source.bytes {
        return Ok(false);
    }
    let bytes = fs::read(path)
        .map_err(|err| file_io_report(path, "failed to read staged snapshot transcript", err))?;
    Ok(sha256_hex(&bytes) == staged_source.sha256)
}

fn parse_snapshot_duration_secs(value: &str) -> MietteResult<u64> {
    if let Some(secs) = rhei_validator::parse_duration_secs(value) {
        return Ok(secs);
    }
    let Some(days) = value.strip_suffix('d').and_then(|n| n.parse::<u64>().ok()) else {
        return Err(miette!("invalid duration '{value}' (expected e.g. 7d, 4h, 30m, 10s)"));
    };
    days.checked_mul(86_400).ok_or_else(|| miette!("duration '{value}' is too large"))
}

fn snapshot_age_secs(record: &SnapshotRecord, now: std::time::SystemTime) -> Option<u64> {
    let created = parse_rfc3339_utc(&record.created_at)
        .or_else(|| record.path.metadata().ok().and_then(|metadata| metadata.modified().ok()))?;
    now.duration_since(created).ok().map(|duration| duration.as_secs())
}

fn parse_rfc3339_utc(value: &str) -> Option<std::time::SystemTime> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<u32>().ok()?;
    let day = value.get(8..10)?.parse::<u32>().ok()?;
    let hour = value.get(11..13)?.parse::<u64>().ok()?;
    let minute = value.get(14..16)?.parse::<u64>().ok()?;
    let second = value.get(17..19)?.parse::<u64>().ok()?;
    if value.get(19..20)? != "Z" || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let secs = days
        .checked_mul(86_400)?
        .checked_add(hour.checked_mul(3_600)? as i64)?
        .checked_add(minute.checked_mul(60)? as i64)?
        .checked_add(second as i64)?;
    if secs < 0 {
        return None;
    }
    Some(std::time::UNIX_EPOCH + Duration::from_secs(secs as u64))
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if month == 0 || month > 12 || day == 0 || day > 31 {
        return None;
    }
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_i = i64::from(month);
    let doy = (153 * (month_i + if month_i > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn is_snapshot_orphaned(record: &SnapshotRecord, ctx: &SnapshotCommandContext) -> bool {
    let task_exists = flatten_tasks(&ctx.loaded.rhei)
        .into_iter()
        .any(|task| task.id.to_string() == record.task_id);
    if !task_exists {
        return true;
    }
    if !ctx.machine.states.contains_key(&record.emitting_state) {
        return true;
    }
    let Ok(slugs) =
        effective_target_slugs_for_state(&ctx.machine, &record.emitting_state, &ctx.settings)
    else {
        return true;
    };
    slugs.is_empty() || !slugs.contains(&record.target_slug)
}

fn effective_target_slugs_for_state(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
) -> MietteResult<BTreeSet<String>> {
    let invocations =
        resolve_agent_invocations(machine, state_name, settings, &default_run_options())?;
    let mut slugs = BTreeSet::new();
    for invocation in invocations {
        if let Some(slug) = resolved_agent_target_slug(&invocation) {
            slugs.insert(slug);
        }
    }
    Ok(slugs)
}

fn resolved_agent_target_slug(resolved: &ResolvedAgent) -> Option<String> {
    if let Some(target) = resolved.target.as_ref() {
        return Some(target.slug());
    }
    let provider = resolved.model_provider.as_deref()?;
    let model = resolved.model_name.as_deref().or(resolved.model.as_deref())?;
    let mut selector = resolved.agent.id().to_string();
    if let Some(mode) = resolved.mode.as_deref() {
        selector.push('[');
        selector.push_str(mode);
        selector.push(']');
    }
    selector.push(':');
    selector.push_str(provider);
    selector.push(':');
    selector.push_str(model);
    Some(slugify_target_value(&selector))
}

struct HeldRunLock {
    file: fs::File,
}

fn acquire_run_lock(workspace_root: &Path) -> MietteResult<HeldRunLock> {
    let file = open_run_lock_file(workspace_root)?;
    file.lock_exclusive().map_err(|err| {
        file_io_report(&workspace_root.join(".rhei/run.lock"), "failed to acquire run lock", err)
    })?;
    Ok(HeldRunLock { file })
}

fn run_lock_is_held(workspace_root: &Path) -> MietteResult<bool> {
    Ok(try_acquire_run_lock(workspace_root)?.is_none())
}

fn try_acquire_run_lock(workspace_root: &Path) -> MietteResult<Option<HeldRunLock>> {
    let file = open_run_lock_file(workspace_root)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(HeldRunLock { file })),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(file_io_report(
            &workspace_root.join(".rhei/run.lock"),
            "failed to inspect run lock",
            err,
        )),
    }
}

fn open_run_lock_file(workspace_root: &Path) -> MietteResult<fs::File> {
    let rhei_dir = workspace_root.join(".rhei");
    fs::create_dir_all(&rhei_dir)
        .map_err(|err| file_io_report(&rhei_dir, "failed to create .rhei directory", err))?;
    let path = rhei_dir.join("run.lock");
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|err| file_io_report(&path, "failed to open run lock", err))
}

impl Drop for HeldRunLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}
