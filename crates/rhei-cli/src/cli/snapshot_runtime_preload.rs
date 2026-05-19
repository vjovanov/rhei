
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
    override_selection: Option<&SnapshotOverrideRunSelection>,
    opts: &RunOptions,
) -> MietteResult<SnapshotPreload> {
    let mut preload = SnapshotPreload::default();
    let declares_inherit = machine
        .states
        .get(current_state)
        .and_then(|state| state.snapshot.as_ref())
        .and_then(|snapshot| snapshot.inherit.as_ref())
        .is_some();

    let target_slug = if declares_inherit {
        Some(snapshot_target_slug_or_err(resolved)?)
    } else {
        resolved_agent_target_slug(resolved)
    };
    let Some(target_slug) = target_slug else {
        return Ok(preload);
    };
    let override_applies = snapshot_override_applies_to_invocation(
        override_selection,
        task,
        &target_slug,
        opts.snapshot_override_ref().is_some(),
    );
    if override_applies && !declares_inherit {
        return Err(miette!(
            "--from-snapshot requires the target state '{}' to declare snapshot.inherit; --override-inherit does not bypass that authored contract",
            current_state
        ));
    }
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
    if override_applies && compat == "none" && !opts.override_inherit() {
        return Err(miette!(
            "--from-snapshot cannot override snapshot.inherit '{}' because compat: none disables authored preload; pass --override-inherit to bypass compatibility checks",
            inherit.name
        ));
    }
    if compat == "none" && !opts.override_inherit() {
        eprintln!("info: snapshot preload disabled by compat: none");
        return Ok(preload);
    }

    let cache_root = snapshot_cache_dir(settings, workspace_root);
    let source = if override_applies {
        let reference = opts.snapshot_override_ref().ok_or_else(|| {
            miette!("internal error: snapshot override selected without a reference")
        })?;
        let loaded = load_plan(input)?;
        let ctx = SnapshotCommandContext {
            workspace_root: workspace_root.to_path_buf(),
            cache_root: cache_root.clone(),
            loaded,
            machine: machine.clone(),
            settings: settings.clone(),
        };
        let record = resolve_snapshot_ref(&ctx, reference, opts.snapshot_target_selector(), None)?;
        if !opts.override_inherit() {
            validate_snapshot_override_contract(
                &cache_root,
                task,
                current_state,
                inherit,
                &target_slug,
                visit_count,
                &record,
                resolved,
            )?;
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
    if !snapshot_preload_session_supported(session) {
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

fn snapshot_override_applies_to_invocation(
    override_selection: Option<&SnapshotOverrideRunSelection>,
    task: &rhei_core::ast::Task,
    target_slug: &str,
    has_override_ref: bool,
) -> bool {
    if !has_override_ref {
        return false;
    }
    override_selection.is_none_or(|selection| {
        selection.task_id == task.id.to_string() && selection.target_slug == target_slug
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_contract(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    record: &SnapshotRecord,
    resolved: &ResolvedAgent,
) -> MietteResult<()> {
    if record.snapshot_name != inherit.name {
        return Err(miette!(
            "--from-snapshot selected snapshot name '{}', but snapshot.inherit requires '{}'",
            record.snapshot_name,
            inherit.name
        ));
    }
    let task_id = task.id.to_string();
    match inherit.from_axis.as_deref().unwrap_or("self") {
        "self" => {
            if record.task_id != task_id {
                return Err(miette!(
                    "--from-snapshot selected task '{}', but snapshot.inherit.from: self requires task '{}'",
                    record.task_id,
                    task_id
                ));
            }
            if record.emitting_state == current_state && record.visit >= visit_count {
                return Err(miette!(
                    "--from-snapshot selected {} from the current or future visit; snapshot.inherit.from: self only permits prior visits",
                    record.display_ref()
                ));
            }
        }
        "ancestor" => {
            let ancestors = ancestor_task_ids(&task_id);
            if !ancestors.iter().any(|ancestor| ancestor == &record.task_id) {
                return Err(miette!(
                    "--from-snapshot selected task '{}', but snapshot.inherit.from: ancestor requires an ancestor of task '{}'",
                    record.task_id,
                    task_id
                ));
            }
        }
        other => {
            return Err(miette!(
                "unsupported snapshot.inherit.from '{}' while validating --from-snapshot",
                other
            ));
        }
    }

    if let Some(select) = inherit.select.as_ref() {
        if let Some(state) = select.state.as_deref() {
            if record.emitting_state != state {
                return Err(miette!(
                    "--from-snapshot selected emitting state '{}', but snapshot.inherit.select.state requires '{}'",
                    record.emitting_state,
                    state
                ));
            }
        }
        if let Some(target) = select.target.as_deref() {
            let required_target = if target == "same" { target_slug } else { target };
            if record.target_slug != required_target {
                return Err(miette!(
                    "--from-snapshot selected target '{}', but snapshot.inherit.select.target requires '{}'",
                    record.target_slug,
                    required_target
                ));
            }
        }
        if let Some(visit) = select.visit.as_ref() {
            validate_snapshot_override_visit(
                cache_root,
                task,
                current_state,
                inherit,
                target_slug,
                visit_count,
                record,
                visit,
            )?;
        } else {
            validate_snapshot_override_default_visit(
                cache_root,
                task,
                current_state,
                inherit,
                target_slug,
                visit_count,
                record,
            )?;
        }
        if let Some(generation) = select.generation.as_ref() {
            validate_snapshot_override_generation(
                cache_root,
                task,
                current_state,
                inherit,
                target_slug,
                visit_count,
                record,
                generation,
            )?;
        } else if !record.is_current {
            return Err(miette!(
                "--from-snapshot selected {}, but snapshot.inherit.select.generation defaults to current",
                record.display_ref()
            ));
        }
    } else {
        validate_snapshot_override_default_visit(
            cache_root,
            task,
            current_state,
            inherit,
            target_slug,
            visit_count,
            record,
        )?;
        if !record.is_current {
            return Err(miette!(
                "--from-snapshot selected {}, but snapshot.inherit.select.generation defaults to current",
                record.display_ref()
            ));
        }
    }

    if inherit.compat.as_deref().unwrap_or("native") == "native"
        && !snapshot_record_native_compatible(record, resolved)
    {
        return Err(miette!(
            "--from-snapshot selected snapshot {} is not native-compatible with agent '{}'",
            record.display_ref(),
            resolved.agent.id()
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_visit(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    record: &SnapshotRecord,
    visit: &YamlValue,
) -> MietteResult<()> {
    if let Some(required_visit) = yaml_selector_u64(visit) {
        if record.visit != required_visit {
            return Err(miette!(
                "--from-snapshot selected visit {}, but snapshot.inherit.select.visit requires {}",
                record.visit,
                required_visit
            ));
        }
    } else if yaml_selector_string(visit) == Some("latest") {
        let candidates = snapshot_override_contract_candidates(
            cache_root,
            task,
            current_state,
            inherit,
            target_slug,
            visit_count,
        )?;
        let latest_visit = candidates.iter().map(|candidate| candidate.visit).max();
        if latest_visit != Some(record.visit) {
            return Err(miette!(
                "--from-snapshot selected visit {}, but snapshot.inherit.select.visit requires latest visit {}",
                record.visit,
                latest_visit.unwrap_or(record.visit)
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_default_visit(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    record: &SnapshotRecord,
) -> MietteResult<()> {
    let candidates = snapshot_override_contract_candidates(
        cache_root,
        task,
        current_state,
        inherit,
        target_slug,
        visit_count,
    )?;
    let latest_visit = candidates.iter().map(|candidate| candidate.visit).max();
    if latest_visit != Some(record.visit) {
        return Err(miette!(
            "--from-snapshot selected visit {}, but snapshot.inherit.select.visit defaults to latest visit {}",
            record.visit,
            latest_visit.unwrap_or(record.visit)
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_generation(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    record: &SnapshotRecord,
    generation: &YamlValue,
) -> MietteResult<()> {
    if let Some(required_generation) = yaml_selector_u64(generation) {
        if record.generation != required_generation {
            return Err(miette!(
                "--from-snapshot selected generation {}, but snapshot.inherit.select.generation requires {}",
                record.generation,
                required_generation
            ));
        }
    } else if yaml_selector_string(generation) == Some("latest") {
        let mut candidates = snapshot_override_contract_candidates(
            cache_root,
            task,
            current_state,
            inherit,
            target_slug,
            visit_count,
        )?;
        candidates.retain(|candidate| candidate.visit == record.visit);
        let latest_generation = candidates.iter().map(|candidate| candidate.generation).max();
        if latest_generation != Some(record.generation) {
            return Err(miette!(
                "--from-snapshot selected generation {}, but snapshot.inherit.select.generation requires latest generation {}",
                record.generation,
                latest_generation.unwrap_or(record.generation)
            ));
        }
    } else if yaml_selector_string(generation) == Some("current") && !record.is_current {
        return Err(miette!(
            "--from-snapshot selected {}, but snapshot.inherit.select.generation requires current",
            record.display_ref()
        ));
    }
    Ok(())
}

fn snapshot_override_contract_candidates(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
) -> MietteResult<Vec<SnapshotRecord>> {
    let mut scoped = read_snapshot_records(cache_root)?
        .into_iter()
        .filter(|candidate| candidate.snapshot_name == inherit.name)
        .filter(|candidate| candidate.produced_by == "orchestrator")
        .filter(|candidate| match inherit.from_axis.as_deref().unwrap_or("self") {
            "self" => {
                candidate.task_id == task.id.to_string()
                    && !(candidate.emitting_state == current_state && candidate.visit >= visit_count)
            }
            "ancestor" => ancestor_task_ids(&task.id.to_string())
                .iter()
                .any(|ancestor| ancestor == &candidate.task_id),
            _ => false,
        })
        .collect::<Vec<_>>();
    if let Some(select) = inherit.select.as_ref() {
        if let Some(state) = select.state.as_deref() {
            scoped.retain(|candidate| candidate.emitting_state == state);
        }
        if let Some(target) = select.target.as_deref() {
            let required_target = if target == "same" { target_slug } else { target };
            scoped.retain(|candidate| candidate.target_slug == required_target);
        }
        if let Some(visit) = select.visit.as_ref().and_then(yaml_selector_u64) {
            scoped.retain(|candidate| candidate.visit == visit);
        }
    }
    Ok(scoped)
}
