
/// Execute the `run` subcommand: advance tasks through the state machine
/// in dependency order.
///
/// In agent mode (the default when an agent is configured), spawns coding
/// agents for each task. In callback-only mode (`--no-agent`), advances
/// tasks through transition callbacks only.
fn run_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    opts: RunOptions,
) -> MietteResult<()> {
    // A Panta project has its own execution command. §FS-rhei-panta.6.2
    if workspace::panta_project_dir(input).is_some() {
        return Err(miette!(
            "'{}' is a Panta project. Use `rhei panta run` to instantiate and run its rheis, or target an individual rhei.",
            input.display()
        ));
    }
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    reject_panta_mutation(&loaded, "run")?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;
    let _run_lock = if opts.dry_run() { None } else { Some(acquire_run_lock(&workspace_root)?) };

    // Warn if --parallel > 1 on single-file plans.
    let is_workspace = workspace::is_workspace(input);
    let effective_parallel = if opts.parallel() > 1 && !is_workspace {
        eprintln!(
            "warning: --parallel > 1 is not supported for single-file plans (risk of \
             conflicting edits). Falling back to sequential execution."
        );
        1
    } else {
        opts.parallel()
    };

    // Initial validation pass.
    let mut report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    report.errors.extend(validate_machine_settings_references(&machine, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &machine));
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    let use_standalone_mode =
        should_use_agent_mode(&loaded.rhei, &machine, &settings, &opts, &workspace_root)?;

    if use_standalone_mode {
        run_agent_mode(input, &machine, &callback_paths, &settings, &opts, effective_parallel)
    } else {
        run_callback_mode(input, &machine, &callback_paths, &opts, effective_parallel)
    }
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

    for task in find_runnable_tasks(rhei, machine, workspace_root) {
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
            let invocations = resolve_agent_invocations(machine, &state_name, settings, opts)?;
            if !invocations.is_empty() || state_declares_autonomous_agent_work(def) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
