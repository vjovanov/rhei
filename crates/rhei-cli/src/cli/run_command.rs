
/// Execute the `run` subcommand: advance tasks through the state machine
/// in dependency order.
///
/// In agent mode (the default when an agent is configured), spawns coding
/// agents for each task. In callback-only mode (`--no-agent`), advances
/// tasks through transition callbacks only.
/// A file that owns more than one ticket, if any. Parallel scheduling may run
/// those tickets' agents concurrently against one checkout. §FS-rhei-run.2.5
fn shared_task_file(loaded: &LoadedPlan) -> Option<&Path> {
    let mut counts: BTreeMap<&Path, usize> = BTreeMap::new();
    for path in loaded.task_sources.values() {
        *counts.entry(path.as_path()).or_default() += 1;
    }
    counts.into_iter().find(|(_, count)| *count > 1).map(|(path, _)| path)
}

fn run_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    opts: RunOptions,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let rhei_scope = resolve_rhei_scope(&loaded, opts.rhei_scope())?;
    report_panta_scope_narrowed(&loaded, "run", &rhei_scope);
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;
    let _run_lock = if opts.dry_run() { None } else { Some(acquire_run_lock(&workspace_root)?) };
    // §FS-rhei-run.3.1: detect subprocess commits that leave run-owned state dirty.
    let git_consistency =
        RunGitConsistencyGuard::capture(&workspace_root, input, !opts.dry_run());

    // Warn if --parallel > 1 on single-file plans.
    let multi_file = workspace::is_workspace(input) || loaded.is_panta_project();
    let effective_parallel = if opts.parallel() > 1 && !multi_file {
        eprintln!(
            "warning: --parallel > 1 is not supported for single-file plans (risk of \
             conflicting edits). Falling back to sequential execution."
        );
        1
    } else {
        // A project keeps parallelism, but two tickets of one rhei file are
        // still concurrent work against a single checkout — say so instead
        // of silently dropping the single-file warning. §FS-rhei-run.2.5
        if opts.parallel() > 1 {
            if let Some(shared) = shared_task_file(&loaded) {
                eprintln!(
                    "warning: --parallel > 1 schedules tickets from the same rhei file \
                     concurrently ({}); plan-file writes serialize on the file lock, but \
                     agents may still collide in the shared checkout.",
                    shared.display()
                );
            }
        }
        opts.parallel()
    };

    // Initial validation pass.
    let mut report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    report.errors.extend(validate_machine_settings_references(&machine, &settings));
    report
        .errors
        .extend(validate_task_execution_override_settings_references(&loaded.rhei, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &machine));
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    let use_standalone_mode =
        should_use_agent_mode(&loaded.rhei, &machine, &settings, &opts, &workspace_root)?;

    let result = if use_standalone_mode {
        run_agent_mode(input, &machine, &callback_paths, &settings, &opts, effective_parallel)
    } else {
        run_callback_mode(input, &machine, &callback_paths, &opts, effective_parallel)
    };
    result?;
    git_consistency.verify_after_success()
}

fn should_use_agent_mode(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
) -> MietteResult<bool> {
    if !opts.no_agent()
        && machine.states.values().any(|def| {
            !def.terminal && !def.gating && state_declares_autonomous_agent_work(def)
        })
    {
        return Ok(true);
    }

    for task in narrow_to_rhei_scope(
        find_runnable_tasks(rhei, machine, workspace_root),
        &rhei_scope_set(opts.rhei_scope()),
    ) {
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let Some(def) = machine.states.get(&state_name) else {
            continue;
        };
        if def.terminal || def.gating {
            continue;
        }
        if def.program.is_some() && !opts.no_program() {
            return Ok(true);
        }
        if !opts.no_agent() {
            let invocations =
                resolve_agent_invocations_for_task(machine, &state_name, settings, opts, Some(task))?;
            if !invocations.is_empty() || state_declares_autonomous_agent_work(def) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
