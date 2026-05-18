fn snapshot_continue_command(
    ctx: &SnapshotCommandContext,
    reference: &str,
    target: Option<&str>,
    generation: Option<u64>,
    no_capture: bool,
) -> MietteResult<()> {
    if run_lock_is_held(&ctx.workspace_root)? {
        return Err(miette!(
            "rhei snapshot continue cannot run while .rhei/run.lock is held; stop the run first"
        ));
    }
    let record = resolve_snapshot_ref(ctx, reference, target, generation)?;
    if record.completion == "timeout" {
        eprintln!(
            "warning: snapshot {} completed by timeout; the native transcript may be truncated",
            record.display_ref()
        );
    }
    let agent_id = record
        .manifest
        .get("target")
        .and_then(|target| target.get("resolved"))
        .and_then(|resolved| resolved.get("agent"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let profile = ctx.settings.agents.get(agent_id).ok_or_else(|| {
        miette!("unsupported-snapshot-session: agent '{agent_id}' is not configured")
    })?;
    if !profile_supports_interactive_continue(&profile.session) {
        return Err(miette!(
            "unsupported-snapshot-session: agent '{}' does not expose a resume strategy, session layout, and interactive continuation profile",
            agent_id
        ));
    }
    let _ = no_capture;
    Err(miette!(
        "unsupported-snapshot-session: interactive snapshot continuation transport is deferred until phase 6"
    ))
}

fn profile_supports_interactive_continue(session: &Option<serde_json::Value>) -> bool {
    let Some(session) = session.as_ref() else {
        return false;
    };
    let has_interactive = session.get("interactive").is_some();
    profile_has_snapshot_preload(&Some(session.clone())) && has_interactive
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
    let file = open_run_lock_file(workspace_root)?;
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = file.unlock();
            Ok(false)
        }
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
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
        let _ = self.file.unlock();
    }
}
