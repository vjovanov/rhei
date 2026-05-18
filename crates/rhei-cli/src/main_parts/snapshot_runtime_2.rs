
/// Orchestration hook for snapshot inheritance preload, invoked before
/// spawning the agent subprocess for a state that declares
/// `snapshot.inherit:`.
///
/// Per `docs/functional-spec/rhei-run.spec.md` § Execution Loop step 3 and
/// `docs/functional-spec/rhei-snapshots.spec.md` § 10.1 Spawn-Time Preload,
/// the orchestrator resolves the source snapshot (honoring `--from-snapshot`,
/// `--override-inherit`, `--task`, and `--target` overrides), evaluates
/// `compat:`, applies the agent's `ResumeStrategy` / `ForkStrategy`, and
/// stages the session into the inheritor's generation directory before the
/// subprocess starts.
///
/// The actual preload is owned by the impl-rhei-snapshots task; this hook
/// pins the call site so the orchestration ordering is encoded in code, not
/// just in the spec text. Once impl-rhei-snapshots delivers the snapshot
/// module, the body of this function calls into that module and may return a
/// `missing-snapshot`, `incompatible-snapshot`, or
/// `unsupported-snapshot-session` error per spec § 10.1.
#[allow(clippy::too_many_arguments)]
fn preload_snapshot_inherit_before_spawn(
    input: &Path,
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
    resolved: &ResolvedAgent,
    settings: &RheiSettings,
    visit_count: u64,
    opts: &RunOptions,
) -> MietteResult<SnapshotPreload> {
    let mut preload = SnapshotPreload::default();
    let target_slug = snapshot_target_slug_or_err(resolved)?;
    if let Some(session) = snapshot_session(resolved) {
        if let Some(flag) = snapshot_session_string(session, "session_dir_flag") {
            let dir = snapshot_session_dir(
                workspace_root,
                &task.id.to_string(),
                current_state,
                &target_slug,
            );
            fs::create_dir_all(&dir).map_err(|err| {
                file_io_report(&dir, "failed to create snapshot session dir", err)
            })?;
            preload.extra_args.push(flag);
            preload.extra_args.push(dir.display().to_string());
            preload.session_dir = Some(dir);
        }
    }

    if opts.snapshot_override_ref().is_some() {
        let declares_inherit = machine
            .states
            .get(current_state)
            .and_then(|state| state.snapshot.as_ref())
            .and_then(|snapshot| snapshot.inherit.as_ref())
            .is_some();
        if !declares_inherit {
            return Err(miette!(
                "--from-snapshot requires the target state '{}' to declare snapshot.inherit; --override-inherit does not bypass that authored contract",
                current_state
            ));
        }
    }

    let Some(inherit) = machine
        .states
        .get(current_state)
        .and_then(|state| state.snapshot.as_ref())
        .and_then(|snapshot| snapshot.inherit.as_ref())
    else {
        return Ok(preload);
    };
    let required = inherit.required.unwrap_or(false);
    let compat = inherit.compat.as_deref().unwrap_or("native");
    if compat == "none" && !opts.override_inherit() {
        eprintln!("info: snapshot preload disabled by compat: none");
        return Ok(preload);
    }

    let cache_root = snapshot_cache_dir(settings, workspace_root);
    let source = if let Some(reference) = opts.snapshot_override_ref() {
        let loaded = load_plan(input)?;
        let ctx = SnapshotCommandContext {
            workspace_root: workspace_root.to_path_buf(),
            cache_root: cache_root.clone(),
            loaded,
            machine: machine.clone(),
            settings: settings.clone(),
        };
        let record = resolve_snapshot_ref(&ctx, reference, opts.snapshot_target_selector(), None)?;
        if let Some(task_selector) = opts.snapshot_task_selector() {
            if record.task_id != task_selector {
                return Err(miette!(
                    "--task selected snapshot task '{}', but override resolved task '{}'",
                    task_selector,
                    record.task_id
                ));
            }
        }
        Some(record)
    } else {
        resolve_inherit_snapshot_source(
            &cache_root,
            task,
            current_state,
            inherit,
            &target_slug,
            visit_count,
        )?
    };

    let Some(source) = source else {
        if required {
            return Err(miette!(
                "missing-snapshot: no snapshot found for inherit '{}'",
                inherit.name
            ));
        }
        eprintln!("warning: no snapshot found for inherit: {}; running cold", inherit.name);
        return Ok(preload);
    };
    if source.completion == "timeout" && !opts.override_inherit() {
        if required {
            return Err(miette!(
                "incompatible-snapshot: selected snapshot {} completed by timeout and is not preloadable",
                source.display_ref()
            ));
        }
        eprintln!(
            "warning: timed-out snapshot {} is not preloadable; running cold",
            source.display_ref()
        );
        return Ok(preload);
    }
    if compat == "native"
        && !opts.override_inherit()
        && !snapshot_record_native_compatible(&source, resolved)
    {
        if required {
            return Err(miette!(
                "incompatible-snapshot: selected snapshot {} is not native-compatible with agent '{}'",
                source.display_ref(),
                resolved.agent.id()
            ));
        }
        eprintln!(
            "warning: preload skipped: incompatible agent for {}; running cold",
            source.display_ref()
        );
        return Ok(preload);
    }
    let Some(session) = snapshot_session(resolved) else {
        if required {
            return Err(miette!(
                "unsupported-snapshot-session: agent '{}' has no supported snapshot preload strategy",
                resolved.agent.id()
            ));
        }
        eprintln!(
            "warning: agent '{}' has no supported snapshot preload strategy; running cold",
            resolved.agent.id()
        );
        return Ok(preload);
    };
    if !snapshot_resume_supported(session) && snapshot_strategy_flag(session, "fork").is_none() {
        if required {
            return Err(miette!(
                "unsupported-snapshot-session: agent '{}' has no supported snapshot preload strategy",
                resolved.agent.id()
            ));
        }
        eprintln!(
            "warning: agent '{}' has no supported snapshot preload strategy; running cold",
            resolved.agent.id()
        );
        return Ok(preload);
    }
    if let Some(reason) = snapshot_cache_benefit_reason(&source, resolved) {
        eprintln!(
            "info: snapshot {} is native-compatible but may not be cache-beneficial: {}",
            source.display_ref(),
            reason
        );
    }
    if let Some(flag) = snapshot_strategy_flag(session, "fork") {
        preload.extra_args.push(flag);
        preload.extra_args.push(source.transcript_path().display().to_string());
    } else if let Some(flag) = snapshot_strategy_flag(session, "resume") {
        let session_id = source
            .manifest
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        preload.extra_args.push(flag);
        preload.extra_args.push(session_id.to_string());
    }
    if let Some(session_dir) = preload.session_dir.as_ref() {
        let ext = source
            .manifest
            .get("session_layout")
            .and_then(snapshot_layout_ext)
            .unwrap_or_else(|| "jsonl".to_string());
        let target = session_dir.join(format!(
            "{}.{}",
            source
                .manifest
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("source"),
            ext
        ));
        fs::copy(source.transcript_path(), &target).map_err(|err| {
            file_io_report(&target, "failed to stage snapshot transcript for preload", err)
        })?;
    }
    preload.parent_ref = Some(snapshot_parent_ref(&source));
    Ok(preload)
}
