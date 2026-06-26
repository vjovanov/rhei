#[allow(clippy::too_many_arguments)]
fn ensure_state_inputs_exist(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
    context: &str,
) -> MietteResult<()> {
    for artifact in &state_def.inputs {
        if artifact.optional {
            continue;
        }
        let (relative, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            task_id,
            state_name,
            visit_count,
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
        );
        if artifact_relative_path_escapes_root(&relative) {
            return Err(miette!(
                "{context}\nInput artifact '{}' expands to '{}' which escapes the workspace root",
                artifact.name,
                relative
            ));
        }
        if !path.exists() {
            return Err(miette!(
                "{context}\nMissing required input artifact: {} ({})",
                artifact.name,
                relative
            ));
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn ensure_state_outputs_exist(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
) -> MietteResult<()> {
    for artifact in &state_def.outputs {
        let (relative, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            task_id,
            state_name,
            visit_count,
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
        );
        if artifact_relative_path_escapes_root(&relative) {
            return Err(miette!(
                "Task {} cannot leave state {}.\nOutput artifact '{}' expands to '{}' which escapes the workspace root",
                task_id,
                state_name,
                artifact.name,
                relative
            ));
        }
        if !path.exists() {
            return Err(miette!(
                "Task {} cannot leave state {}.\nMissing required output artifact: {} ({})",
                task_id,
                state_name,
                artifact.name,
                relative
            ));
        }
    }

    Ok(())
}

/// Execute the `transition` subcommand: atomic compare-and-swap state change.
///
/// Acquires an exclusive file lock, verifies the task's current state matches
/// `from`, validates the transition against the state machine, rewrites the
/// `**State:**` line, and writes the file atomically (temp + rename).
fn transition_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    reject_panta_mutation(&loaded, "transition")?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;

    let task_file = if workspace::is_workspace(input) {
        loaded.task_file(task_id_str, input)
    } else {
        input.to_path_buf()
    };
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
        from,
        to,
        no_callbacks,
    )?;

    let root = result_workspace_root(input, &task_file);
    record_transition_result(&root, &task_file, &machine, task_id_str, from, &effective_to, None)?;

    println!("Task {} transitioned: '{}' → '{}'", task_id_str, from, effective_to);
    Ok(())
}

/// Core transition logic shared by `transition` and `run` commands.
///
/// Validates states and transition legality, acquires an exclusive file lock,
/// performs compare-and-swap verification, executes callbacks, and atomically
/// rewrites the plan file. Returns an error if any step fails.
///
/// `task_file` is the specific file to lock and rewrite (for directory
/// workspaces this is the file inside `tasks/` that contains the task;
/// for single-file plans it equals `plan_path`).
///
/// `plan_path` is the top-level plan path used in callback context.
fn execute_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<String> {
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin::default(),
    )
}
