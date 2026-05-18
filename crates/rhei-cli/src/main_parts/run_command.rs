
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
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
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

    let mut use_standalone_mode = false;
    for def in machine.states.values() {
        if def.terminal || def.gating {
            continue;
        }
        if state_declares_enabled_autonomous_execution(def, &opts) {
            use_standalone_mode = true;
            break;
        }
    }

    if use_standalone_mode {
        run_agent_mode(input, &machine, &callback_paths, &settings, &opts, effective_parallel)
    } else {
        run_callback_mode(input, &machine, &callback_paths, &opts, effective_parallel)
    }
}
