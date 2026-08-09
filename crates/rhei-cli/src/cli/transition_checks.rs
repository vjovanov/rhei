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
            // Pre-qualification artifacts are keyed by the rhei-local id;
            // when one exists, name it so the fix is one rename away. §FS-rhei-panta.6
            let local_id = rhei_local_id_str(task_id);
            let legacy_hint = if local_id != task_id && artifact.path.contains("{task_id}") {
                let (legacy_relative, legacy_path) = resolve_artifact_path(
                    workspace_root,
                    artifact,
                    local_id,
                    state_name,
                    visit_count,
                    target,
                    model,
                    model_provider,
                    model_name,
                    agent,
                    agent_mode,
                );
                if legacy_path.exists() {
                    format!(
                        "\nA pre-qualification artifact exists at '{legacy_relative}'. \
                         Ticket ids are now project-qualified; rename it to '{relative}' \
                         to keep this run's history."
                    )
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            return Err(miette!(
                "{context}\nMissing required input artifact: {} ({}){}",
                artifact.name,
                relative,
                legacy_hint
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
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    // No `--rhei` on this command: the explicit ticket target is the scope,
    // narrowed by the rhei the invocation was pointed at. §FS-rhei-panta.6
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let task_id_str = &resolve_cli_task_id(&loaded, task_id_str, &scope)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;

    let route = loaded.task_route(task_id_str, input);

    let effective_to = execute_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        &callback_paths,
        &machine,
        &route.local_id,
        from,
        to,
        no_callbacks,
    )?;

    append_result_entry(&route.execution_root, task_id_str, from, &effective_to, None)?;

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

/// Append the command-owned central audit entry for a state change that was
/// already applied by the run orchestrator, into the owning rhei's runtime
/// ledger. §FS-rhei-run.3 §FS-rhei-complete.3.1
fn append_transition_audit_entry(
    execution_root: &Path,
    task_id_str: &str,
    from: &str,
    to: &str,
) -> MietteResult<()> {
    append_result_entry(execution_root, task_id_str, from, to, None)
}
