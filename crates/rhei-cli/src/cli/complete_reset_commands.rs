
/// Execute the `complete` subcommand: transition a task to a terminal state,
/// write the central state ledger and result artifact, link it from the task
/// body, and remove the assignee.
///
/// The target terminal state is chosen automatically: the first non-cancelled
/// terminal state reachable from the task's current state via a declared
/// transition. If no such transition exists, the command fails.
fn complete_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    result_msg: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    reject_panta_mutation(&loaded, "complete")?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    // Find the task and its current state.
    let target_id = parse_task_id(task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
    let current_state_raw = task.state.as_str();
    let current_state = normalized_state_name(current_state_raw, &machine);

    // Reject tasks already in a terminal state.
    if is_terminal_state(current_state_raw, &machine) {
        return Err(miette!(
            "Task {} is already in terminal state '{}'",
            task_id_str,
            current_state_raw
        ));
    }
    if machine.states.get(&current_state).map(|def| def.gating).unwrap_or(false) {
        return Err(miette!(
            "Task {} cannot be completed from gating state '{}'; use an explicit human transition",
            task_id_str,
            current_state
        ));
    }

    let open_children = non_terminal_descendants(task, &machine);
    if !open_children.is_empty() {
        return Err(miette!(
            "Task {} cannot be completed while child tasks remain non-terminal.\nOffending children: {}",
            task_id_str,
            open_children.join(", ")
        ));
    }

    // Find the completion target: a non-cancelled terminal state reachable via
    // a single declared transition from the current state.
    let to_state = find_completion_state(&current_state, &machine).ok_or_else(|| {
        miette!(
            "no transition to a terminal state available from '{}' for Task {}",
            current_state_raw,
            task_id_str
        )
    })?;

    // Execute the state transition (compare-and-swap, callbacks, atomic write).
    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };
    let effective_to = execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        &callback_paths,
        &machine,
        task_id_str,
        &current_state,
        &to_state,
        no_callbacks,
    )?;
    if !is_successful_completion_state(&effective_to, &machine) {
        return Err(miette!(
            "Task {} was redirected to '{}', which is not a successful completion state; completion artifacts were not written",
            task_id_str,
            effective_to
        ));
    }

    // Append the completion entry to the result file.
    let root = result_workspace_root(input, &task_file);
    let result_link = format!("runtime/results/{}.md", task_id_str);
    append_result_entry(&root, task_id_str, current_state_raw, &effective_to, Some(result_msg))?;

    // Post-transition: remove assignee and link the result file (first time only).
    rewrite_task_completion(&task_file, task_id_str, task_id_str, &result_link, true)?;

    println!(
        "Task {} completed: '{}' → '{}' ({})",
        task_id_str, current_state_raw, effective_to, result_link
    );

    Ok(())
}

/// Execute the `reset` subcommand: restore every task in the tree to the
/// state machine's initial state.
///
/// For directory workspaces, this also removes the generated `runtime/`
/// directory so logs and artifacts do not survive the reset.
fn reset_command(input: &Path, state_machine_path: Option<&Path>) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    reject_panta_mutation(&loaded, "reset")?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let reset_summary = reset_initial_summary(&loaded.rhei, &resolved.machine)?;

    fn count_nodes(task: &rhei_core::ast::Task) -> usize {
        1 + task.children.iter().map(count_nodes).sum::<usize>()
    }
    let task_count = loaded.rhei.tasks.len();
    let total_nodes: usize = loaded.rhei.tasks.iter().map(count_nodes).sum();
    let descendant_count = total_nodes.saturating_sub(task_count);

    for file in reset_target_files(&loaded, input) {
        reset_plan_file_states(&file, &resolved.machine)?;
    }
    if workspace::is_workspace(input) {
        clear_runtime_metadata_in_file(&input.join("index.rhei.md"), true)?;
    }

    let mut removed_runtime = false;
    let runtime_dir = if workspace::is_workspace(input) {
        Some(input.join("runtime"))
    } else {
        input.parent().map(|p| p.join("runtime"))
    };
    if let Some(runtime_dir) = runtime_dir {
        if runtime_dir.exists() {
            fs::remove_dir_all(&runtime_dir).map_err(|err| {
                file_io_report(&runtime_dir, "failed to remove runtime directory", err)
            })?;
            removed_runtime = true;
        }
    }

    if descendant_count == 0 {
        println!("Reset {} task(s) {}.", task_count, reset_summary);
    } else {
        println!(
            "Reset {} task(s) (and {} descendant task(s)) {}.",
            task_count, descendant_count, reset_summary
        );
    }
    if removed_runtime {
        println!("Removed runtime output.");
    } else {
        println!("No runtime output was present.");
    }

    Ok(())
}

fn reset_initial_summary(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<String> {
    fn collect(
        task: &rhei_core::ast::Task,
        machine: &rhei_validator::StateMachine,
        states: &mut BTreeSet<String>,
    ) -> MietteResult<()> {
        states.insert(initial_state_for_node(machine, &task.kind, task.profile_level())?);
        for child in &task.children {
            collect(child, machine, states)?;
        }
        Ok(())
    }

    let mut states = BTreeSet::new();
    for task in &rhei.tasks {
        collect(task, machine, &mut states)?;
    }

    match states.len() {
        0 => Ok("to resolved initial states".to_string()),
        1 => Ok(format!("to initial state '{}'", states.iter().next().expect("one state"))),
        _ => Ok(format!(
            "to resolved profile initial states ({})",
            states.into_iter().collect::<Vec<_>>().join(", ")
        )),
    }
}

fn initial_state_for_node(
    machine: &rhei_validator::StateMachine,
    kind: &str,
    level: u8,
) -> MietteResult<String> {
    if let Some(profile) = machine.profile_for_node(kind, level) {
        return Ok(profile.initial.clone());
    }
    initial_state_name(machine)
}

fn initial_state_name(machine: &rhei_validator::StateMachine) -> MietteResult<String> {
    let initial_states = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    match initial_states.as_slice() {
        [] => Err(miette!("state machine '{}' does not declare an initial state", machine.name)),
        [initial] => Ok(initial.clone()),
        many => Err(miette!(
            "state machine '{}' declares multiple legacy initial states: {}",
            machine.name,
            many.join(", ")
        )),
    }
}
