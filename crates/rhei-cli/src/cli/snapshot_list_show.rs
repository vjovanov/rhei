fn snapshot_command(
    command: SnapshotCommand,
    state_machine_path: Option<&Path>,
) -> MietteResult<()> {
    match command {
        SnapshotCommand::List { plan, task, name, state, produced_by, orphaned, format } => {
            let ctx = load_snapshot_context(&plan, state_machine_path)?;
            let records = read_snapshot_records(&ctx.cache_root)?;
            let mut rows = Vec::new();
            for record in records {
                if !snapshot_record_matches_filters(
                    &record,
                    task.as_deref(),
                    name.as_deref(),
                    state.as_deref(),
                    produced_by,
                ) {
                    continue;
                }
                let is_orphan = is_snapshot_orphaned(&record, &ctx);
                if orphaned && !is_orphan {
                    continue;
                }
                rows.push((record, is_orphan));
            }
            rows.sort_by(|a, b| {
                a.0.task_id
                    .cmp(&b.0.task_id)
                    .then_with(|| a.0.snapshot_name.cmp(&b.0.snapshot_name))
                    .then_with(|| a.0.emitting_state.cmp(&b.0.emitting_state))
                    .then_with(|| a.0.visit.cmp(&b.0.visit))
                    .then_with(|| a.0.target_slug.cmp(&b.0.target_slug))
                    .then_with(|| a.0.generation.cmp(&b.0.generation))
            });
            print_snapshot_list(rows, format)
        }
        SnapshotCommand::Show { reference, plan } => {
            let ctx = load_snapshot_context(&plan, state_machine_path)?;
            let record = resolve_snapshot_ref(&ctx, &reference, None, None)?;
            print_snapshot_show(&record)
        }
        SnapshotCommand::Gc {
            plan,
            task,
            name,
            older_than,
            keep_generations,
            include_operator,
            orphaned,
            dry_run,
            force,
        } => {
            let ctx = load_snapshot_context(&plan, state_machine_path)?;
            snapshot_gc_command(
                &ctx,
                task.as_deref(),
                name.as_deref(),
                older_than.as_deref(),
                keep_generations,
                include_operator,
                orphaned,
                dry_run,
                force,
            )
        }
        SnapshotCommand::Continue { reference, plan, target, generation, no_capture } => {
            let ctx = load_snapshot_context(&plan, state_machine_path)?;
            snapshot_continue_command(&ctx, &reference, target.as_deref(), generation, no_capture)
        }
    }
}

fn load_snapshot_context(
    plan: &Path,
    state_machine_path: Option<&Path>,
) -> MietteResult<SnapshotCommandContext> {
    let input_buf = normalize_workspace_input(plan);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;
    let cache_root = snapshot_cache_dir(&settings, &workspace_root);
    Ok(SnapshotCommandContext {
        workspace_root,
        cache_root,
        loaded,
        machine: resolved.machine,
        settings,
    })
}

fn snapshot_record_matches_filters(
    record: &SnapshotRecord,
    task: Option<&str>,
    name: Option<&str>,
    state: Option<&str>,
    produced_by: SnapshotProducedByFilter,
) -> bool {
    if task.is_some_and(|value| record.task_id != value) {
        return false;
    }
    if name.is_some_and(|value| record.snapshot_name != value) {
        return false;
    }
    if state.is_some_and(|value| record.emitting_state != value) {
        return false;
    }
    match produced_by {
        SnapshotProducedByFilter::Orchestrator => record.produced_by == "orchestrator",
        SnapshotProducedByFilter::Operator => record.produced_by == "operator",
        SnapshotProducedByFilter::All => true,
    }
}

fn read_snapshot_records(cache_root: &Path) -> MietteResult<Vec<SnapshotRecord>> {
    if !cache_root.exists() {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    collect_manifest_paths(cache_root, cache_root, &mut manifests)
        .map_err(|err| file_io_report(cache_root, "failed to read snapshot cache", err))?;

    let mut records = Vec::new();
    for manifest_path in manifests {
        let raw = fs::read_to_string(&manifest_path).map_err(|err| {
            file_io_report(&manifest_path, "failed to read snapshot manifest", err)
        })?;
        let manifest: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
            miette!("failed to parse snapshot manifest '{}': {err}", manifest_path.display())
        })?;
        if let Some(record) = snapshot_record_from_manifest(cache_root, &manifest_path, manifest)? {
            records.push(record);
        }
    }
    Ok(records)
}

fn collect_manifest_paths(cache_root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if entry.file_name().to_str().is_some_and(|name| name.contains(".tmp-")) {
                continue;
            }
            collect_manifest_paths(cache_root, &path, out)?;
        } else if entry.file_name() == OsStr::new("manifest.json")
            && is_committed_snapshot_manifest(cache_root, &path)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn is_committed_snapshot_manifest(cache_root: &Path, manifest_path: &Path) -> bool {
    let relative = manifest_path.strip_prefix(cache_root).unwrap_or(manifest_path);
    if relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.contains(".tmp-"))
    }) {
        return false;
    }
    manifest_path
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .and_then(|name| name.strip_prefix('g'))
        .is_some_and(|generation| !generation.is_empty() && generation.parse::<u64>().is_ok())
}

fn snapshot_record_from_manifest(
    _cache_root: &Path,
    manifest_path: &Path,
    manifest: serde_json::Value,
) -> MietteResult<Option<SnapshotRecord>> {
    let Some(generation_dir) = manifest_path.parent() else {
        return Ok(None);
    };
    validate_snapshot_manifest_schema(manifest_path, generation_dir, &manifest)?;
    let get_str = |key: &str| manifest.get(key).and_then(serde_json::Value::as_str);
    let task_id = get_str("task_id").unwrap_or_default().to_string();
    let snapshot_name = get_str("snapshot_name").unwrap_or_default().to_string();
    let emitting_state = get_str("emitting_state").unwrap_or_default().to_string();
    let visit = manifest.get("visit").and_then(serde_json::Value::as_u64).unwrap_or(1);
    let generation = manifest.get("generation").and_then(serde_json::Value::as_u64).unwrap_or(1);
    let target_slug = manifest
        .get("target")
        .and_then(|target| target.get("slug"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    if task_id.is_empty()
        || snapshot_name.is_empty()
        || emitting_state.is_empty()
        || target_slug.is_empty()
    {
        return Ok(None);
    }
    let is_current = snapshot_current_points_to(generation_dir);
    let created_at = get_str("created_at").unwrap_or_default().to_string();
    let transcript_bytes =
        manifest.get("transcript_bytes").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let completion = get_str("completion").unwrap_or_default().to_string();
    let produced_by = get_str("produced_by").unwrap_or("orchestrator").to_string();
    validate_snapshot_manifest_completion(manifest_path, &completion, &produced_by)?;
    Ok(Some(SnapshotRecord {
        path: generation_dir.to_path_buf(),
        manifest,
        task_id,
        snapshot_name,
        emitting_state,
        visit,
        target_slug,
        generation,
        created_at,
        transcript_bytes,
        completion,
        produced_by,
        is_current,
    }))
}

fn validate_snapshot_manifest_schema(
    manifest_path: &Path,
    generation_dir: &Path,
    manifest: &serde_json::Value,
) -> MietteResult<()> {
    let version = manifest.get("version").and_then(serde_json::Value::as_u64);
    if version != Some(1) {
        return Err(miette!(
            "invalid snapshot manifest '{}': version must be 1",
            manifest_path.display()
        ));
    }
    for key in [
        "rhei_version",
        "snapshot_name",
        "task_id",
        "emitting_state",
        "created_at",
        "completion",
        "produced_by",
        "declared_provider",
        "declared_model",
        "observed_provider",
        "observed_model",
        "session_id",
        "transcript_path",
        "transcript_sha256",
    ] {
        if manifest.get(key).and_then(serde_json::Value::as_str).is_none() {
            return Err(miette!(
                "invalid snapshot manifest '{}': missing string field '{}'",
                manifest_path.display(),
                key
            ));
        }
    }
    for key in ["visit", "generation", "transcript_bytes"] {
        if manifest
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|value| key != "transcript_bytes" && value == 0)
        {
            return Err(miette!(
                "invalid snapshot manifest '{}': missing positive integer field '{}'",
                manifest_path.display(),
                key
            ));
        }
    }
    let target = manifest.get("target").ok_or_else(|| {
        miette!(
            "invalid snapshot manifest '{}': missing object field 'target'",
            manifest_path.display()
        )
    })?;
    for key in ["selector", "slug"] {
        if target.get(key).and_then(serde_json::Value::as_str).is_none() {
            return Err(miette!(
                "invalid snapshot manifest '{}': missing string field 'target.{}'",
                manifest_path.display(),
                key
            ));
        }
    }
    if !target.get("resolved").is_some_and(serde_json::Value::is_object) {
        return Err(miette!(
            "invalid snapshot manifest '{}': missing object field 'target.resolved'",
            manifest_path.display()
        ));
    }
    let session_layout = manifest.get("session_layout").ok_or_else(|| {
        miette!(
            "invalid snapshot manifest '{}': missing object field 'session_layout'",
            manifest_path.display()
        )
    })?;
    if session_layout.get("kind").and_then(serde_json::Value::as_str).is_none()
        || session_layout.get("ext").and_then(serde_json::Value::as_str).is_none()
    {
        return Err(miette!(
            "invalid snapshot manifest '{}': session_layout requires kind and ext",
            manifest_path.display()
        ));
    }
    if !manifest.get("parent_ref").is_some_and(|value| value.is_null() || value.is_object()) {
        return Err(miette!(
            "invalid snapshot manifest '{}': parent_ref must be object or null",
            manifest_path.display()
        ));
    }
    validate_snapshot_manifest_completion(
        manifest_path,
        manifest.get("completion").and_then(serde_json::Value::as_str).unwrap_or_default(),
        manifest.get("produced_by").and_then(serde_json::Value::as_str).unwrap_or_default(),
    )?;

    let generation = manifest.get("generation").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let visit = manifest.get("visit").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if generation_dir.file_name().and_then(OsStr::to_str) != Some(format!("g{generation}").as_str())
    {
        return Err(miette!(
            "invalid snapshot manifest '{}': generation does not match path",
            manifest_path.display()
        ));
    }
    let Some(target_dir) = generation_dir.parent() else {
        return Ok(());
    };
    let path_target = target_dir.file_name().and_then(OsStr::to_str).unwrap_or_default();
    let path_visit =
        target_dir.parent().and_then(Path::file_name).and_then(OsStr::to_str).unwrap_or_default();
    let path_state = target_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let path_name = target_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let path_task = target_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let target_slug = target.get("slug").and_then(serde_json::Value::as_str).unwrap_or_default();
    if manifest.get("task_id").and_then(serde_json::Value::as_str) != Some(path_task)
        || manifest.get("snapshot_name").and_then(serde_json::Value::as_str) != Some(path_name)
        || manifest.get("emitting_state").and_then(serde_json::Value::as_str) != Some(path_state)
        || path_visit.parse::<u64>().ok() != Some(visit)
        || target_slug != path_target
    {
        return Err(miette!(
            "invalid snapshot manifest '{}': identity fields do not match path",
            manifest_path.display()
        ));
    }
    Ok(())
}

fn validate_snapshot_manifest_completion(
    manifest_path: &Path,
    completion: &str,
    produced_by: &str,
) -> MietteResult<()> {
    match produced_by {
        "orchestrator" if matches!(completion, "success" | "failure" | "timeout") => Ok(()),
        "operator" if matches!(completion, "success" | "failure") => Ok(()),
        "orchestrator" | "operator" => Err(miette!(
            "invalid snapshot manifest '{}': completion '{}' is not valid for produced_by '{}'",
            manifest_path.display(),
            completion,
            produced_by
        )),
        _ => Err(miette!(
            "invalid snapshot manifest '{}': produced_by must be 'orchestrator' or 'operator'",
            manifest_path.display()
        )),
    }
}

fn snapshot_current_points_to(generation_dir: &Path) -> bool {
    let Some(identity_dir) = generation_dir.parent() else {
        return false;
    };
    let current = identity_dir.join("current");
    let Ok(target) = fs::read_link(&current) else {
        return false;
    };
    let resolved = if target.is_absolute() { target } else { identity_dir.join(target) };
    paths_equivalent(&resolved, generation_dir)
}

fn print_snapshot_list(
    rows: Vec<(SnapshotRecord, bool)>,
    format: SnapshotListFormat,
) -> MietteResult<()> {
    match format {
        SnapshotListFormat::Json => {
            let payload: Vec<serde_json::Value> =
                rows.iter().map(|(record, orphaned)| record.to_listing_json(*orphaned)).collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&payload)
                    .map_err(|err| miette!("failed to serialize snapshot list: {err}"))?
            );
        }
        SnapshotListFormat::Text => {
            println!(
                "task\tsnapshot\temitting_state\tvisit\ttarget\tgeneration\tcreated_at\ttranscript_bytes\tcompletion\tproduced_by"
            );
            for (record, _) in rows {
                println!(
                    "{}\t{}\t{}\t{}\t{}\tg{}\t{}\t{}\t{}\t{}",
                    record.task_id,
                    record.snapshot_name,
                    record.emitting_state,
                    record.visit,
                    record.target_slug,
                    record.generation,
                    record.created_at,
                    record.transcript_bytes,
                    record.completion,
                    record.produced_by
                );
            }
        }
    }
    Ok(())
}

fn print_snapshot_show(record: &SnapshotRecord) -> MietteResult<()> {
    println!("manifest:");
    println!(
        "{}",
        serde_json::to_string_pretty(&record.manifest)
            .map_err(|err| miette!("failed to serialize snapshot manifest: {err}"))?
    );
    println!();
    println!("transcript preview:");
    let transcript = record.transcript_path();
    let raw = match fs::read_to_string(&transcript) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            println!("(transcript not found: {})", transcript.display());
            return Ok(());
        }
        Err(err) => return Err(file_io_report(&transcript, "failed to read transcript", err)),
    };
    let lines: Vec<&str> = raw.lines().collect();
    let head = lines.iter().take(12).copied().collect::<Vec<_>>();
    for line in head {
        println!("{line}");
    }
    if lines.len() > 24 {
        println!("...");
        for line in lines.iter().skip(lines.len().saturating_sub(12)) {
            println!("{line}");
        }
    } else {
        for line in lines.iter().skip(12) {
            println!("{line}");
        }
    }
    Ok(())
}
