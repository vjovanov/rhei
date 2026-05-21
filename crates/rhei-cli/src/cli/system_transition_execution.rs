
struct TransitionTaskInfo {
    current_state: String,
    kind: String,
    level: u8,
}

fn task_profile_allows_state(
    machine: &rhei_validator::StateMachine,
    kind: &str,
    level: u8,
    state: &str,
) -> bool {
    machine
        .profile_for_node(kind, level)
        .is_none_or(|profile| profile.allowed.iter().any(|allowed| allowed == state))
}

fn ensure_task_profile_allows_state(
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    kind: &str,
    level: u8,
    state: &str,
) -> MietteResult<()> {
    let Some(profile) = machine.profile_for_node(kind, level) else {
        return Ok(());
    };
    if profile.allowed.iter().any(|allowed| allowed == state) {
        return Ok(());
    }

    Err(miette!(
        "Task {} cannot enter state '{}': state is not allowed by its resolved profile. Profile allows: [{}]",
        task_id_str,
        state,
        profile.allowed.join(", ")
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_transition_with_origin(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
    origin: TransitionOrigin,
) -> MietteResult<String> {
    let task_file = files.task_file;
    let metadata_file = files.metadata_file;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;

    // Validate that both `from` and `to` are valid states.
    if !machine.is_valid_state(from) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!("'{}' is not a valid state. Allowed: [{}]", from, allowed));
    }
    if !machine.is_valid_state(to) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!("'{}' is not a valid state. Allowed: [{}]", to, allowed));
    }

    // Open the file(s) with an exclusive lock for the duration of the operation.
    let metadata_handle = fs::File::open(metadata_file)
        .map_err(|err| file_io_report(metadata_file, "failed to open plan file", err))?;
    metadata_handle
        .lock_exclusive()
        .map_err(|err| file_io_report(metadata_file, "failed to acquire file lock", err))?;
    let task_handle = if task_file == metadata_file {
        None
    } else {
        let handle = fs::File::open(task_file)
            .map_err(|err| file_io_report(task_file, "failed to open plan file", err))?;
        handle
            .lock_exclusive()
            .map_err(|err| file_io_report(task_file, "failed to acquire file lock", err))?;
        Some(handle)
    };

    // Read the raw markdown while holding the locks.
    let metadata_raw = fs::read_to_string(metadata_file)
        .map_err(|err| file_io_report(metadata_file, "failed to read plan file", err))?;
    let task_raw = if task_file == metadata_file {
        metadata_raw.clone()
    } else {
        fs::read_to_string(task_file)
            .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?
    };

    // Parse to validate structure and find the task.
    // Try full plan parse first; fall back to workspace task-file parse.
    let target_id = parse_task_id(task_id_str);
    let task_info = find_task_transition_info(&task_raw, task_file, &target_id, task_id_str)?;
    let current_state_raw = task_info.current_state;
    let current_state = normalized_state_name(&current_state_raw, machine);
    let metadata = if task_file == metadata_file {
        rhei_core::parse(&metadata_raw)
            .map_err(|err| {
                miette!("failed to parse plan for transition metadata: {}", err.message)
            })?
            .metadata
    } else {
        rhei_core::parser::parse_workspace_index(&metadata_raw)
            .map_err(|err| {
                miette!("failed to parse workspace index for transition metadata: {}", err.message)
            })?
            .metadata
    };

    // Compare-and-swap: verify the task's current state matches `from`.
    // This runs before the transition-legality check so a wrong `--from`
    // produces the actionable "task is in state X" error instead of the
    // less informative "transition not allowed" error.
    if current_state != from {
        if let Some(task_handle) = &task_handle {
            let _ = task_handle.unlock();
        }
        let _ = metadata_handle.unlock();
        return Err(miette!(
            "conflict: Task {} is in state '{}', expected '{}'",
            task_id_str,
            current_state_raw,
            from
        ));
    }
    if let Err(err) = ensure_task_profile_allows_state(
        machine,
        task_id_str,
        &task_info.kind,
        task_info.level,
        to,
    ) {
        if let Some(task_handle) = &task_handle {
            let _ = task_handle.unlock();
        }
        let _ = metadata_handle.unlock();
        return Err(err);
    }

    // Now that we know the task really is in `from`, check whether the
    // declared transitions permit `from -> to`.
    let matching_rule =
        machine.transitions().iter().find(|rule| rule.from.0 == from && rule.to.0 == to).or_else(
            || machine.transitions().iter().find(|rule| rule.from.0 == "*" && rule.to.0 == to),
        );
    let Some(matching_rule) = matching_rule else {
        if let Some(task_handle) = &task_handle {
            let _ = task_handle.unlock();
        }
        let _ = metadata_handle.unlock();
        return Err(miette!(
            "transition from '{}' to '{}' is not allowed by the state machine",
            from,
            to
        ));
    };

    let normalized_metadata = ensure_current_state_visit_count(
        metadata.as_ref(),
        &target_id,
        from,
        &current_state_raw,
        machine,
    );
    let metadata_for_checks = normalized_metadata.as_ref().or(metadata.as_ref());

    if !transition_rule_is_applicable(
        matching_rule,
        machine,
        metadata_for_checks,
        &target_id,
        from,
        &current_state_raw,
    )? {
        if let Some(task_handle) = &task_handle {
            let _ = task_handle.unlock();
        }
        let _ = metadata_handle.unlock();
        let reason = describe_blocked_transition(
            matching_rule,
            machine,
            metadata_for_checks,
            &target_id,
            from,
            &current_state_raw,
        );
        let alternatives = applicable_alternatives(
            machine,
            metadata_for_checks,
            &target_id,
            from,
            &current_state_raw,
        );
        let suffix = if alternatives.is_empty() {
            "No other transitions from this state are currently applicable.".to_string()
        } else {
            format!(
                "Currently applicable transitions from '{}': {}.",
                from,
                alternatives.join(", ")
            )
        };
        return Err(miette!(
            "transition from '{}' to '{}' is not currently applicable: {}. {}",
            from,
            to,
            reason,
            suffix
        ));
    }

    let from_state_def = machine
        .states
        .get(from)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", from))?;

    let from_invocations =
        resolve_agent_invocations(machine, from, &settings, &default_run_options())
            .unwrap_or_default();
    let callback_contexts = callback_contexts_for_state(from_state_def, &from_invocations);

    // Parse the plan once for callback-context serialization. Failure here
    // means we fall back to a minimal payload rather than aborting — the
    // transition should still run even if the plan is only partially
    // structured.
    let plan_for_context = rhei_core::parse(&metadata_raw).ok();

    // Accumulated `transitionData` that flows from on_leave callbacks to
    // on_enter. Starts from the engine-seeded payload (e.g. timeout data)
    // and each callback's `data` merges last-write-wins.
    let mut transition_data: serde_json::Value = origin
        .seed_data
        .clone()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    // A callback may request a redirect via `next_state`. The first such
    // request wins; later callbacks are still executed against the original
    // `to` for rejection checks, but their redirects are ignored.
    let mut redirect_next_state: Option<String> = None;

    // Execute on_leave callback before the state change.
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_leave {
            let executor = ShellCallbackExecutor;
            for (model, agent) in callback_contexts {
                let context_json = build_transition_context_json(
                    plan_for_context.as_ref(),
                    &callback_paths.plan_path,
                    task_id_str,
                    from,
                    to,
                    origin.triggered_by.unwrap_or("user"),
                    &transition_data,
                    &callback_paths.working_dir,
                );
                let ctx = CallbackContext {
                    task_id: task_id_str,
                    from_state: from,
                    to_state: to,
                    plan_path: &callback_paths.plan_path,
                    callback_cwd: &callback_paths.working_dir,
                    model,
                    agent,
                    context_json: Some(&context_json),
                };
                let result = executor.execute(cb, &ctx).map_err(|e| miette!("{e}"))?;
                if !result.success {
                    if let Some(task_handle) = &task_handle {
                        let _ = task_handle.unlock();
                    }
                    let _ = metadata_handle.unlock();
                    let message = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "transition rejected by callback".to_string());
                    return Err(miette!(
                        "on_leave callback '{}' rejected the transition: {message}",
                        cb.0
                    ));
                }
                if let Some(data) = result.data.as_ref() {
                    merge_transition_data(&mut transition_data, data);
                }
                if let Some(redirect) = result.next_state.clone() {
                    if redirect_next_state.is_none() {
                        redirect_next_state = Some(redirect);
                    }
                }
            }
        }
    }

    // Resolve redirects before committing state: validate the redirect is a
    // declared transition from the current state. A redirect to the same
    // target is a no-op.
    let (effective_to, effective_rule) = if let Some(redirect) = redirect_next_state.as_deref() {
        if redirect == to {
            (to.to_string(), matching_rule)
        } else if !machine.is_valid_state(redirect) {
            if let Some(task_handle) = &task_handle {
                let _ = task_handle.unlock();
            }
            let _ = metadata_handle.unlock();
            return Err(miette!("on_leave callback redirected to unknown state '{}'", redirect));
        } else if let Err(err) = ensure_task_profile_allows_state(
            machine,
            task_id_str,
            &task_info.kind,
            task_info.level,
            redirect,
        ) {
            if let Some(task_handle) = &task_handle {
                let _ = task_handle.unlock();
            }
            let _ = metadata_handle.unlock();
            return Err(err);
        } else if let Some(rule) =
            machine.transitions().iter().find(|r| r.from.0 == from && r.to.0 == redirect).or_else(
                || machine.transitions().iter().find(|r| r.from.0 == "*" && r.to.0 == redirect),
            )
        {
            (redirect.to_string(), rule)
        } else {
            if let Some(task_handle) = &task_handle {
                let _ = task_handle.unlock();
            }
            let _ = metadata_handle.unlock();
            return Err(miette!(
                "on_leave callback redirected to '{}', but no transition from '{}' to '{}' is declared",
                redirect,
                from,
                redirect
            ));
        }
    } else {
        (to.to_string(), matching_rule)
    };
    let to = effective_to.as_str();
    let matching_rule = effective_rule;

    let to_state_def = machine
        .states
        .get(to)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", to))?;

    let mut updated_metadata =
        update_metadata_for_transition(metadata_for_checks, &target_id, to, machine)
            .or_else(|| normalized_metadata.clone());
    if from_state_def.poll.is_some() && to != from {
        updated_metadata = clear_poll_state_metadata(
            updated_metadata.as_ref().or(metadata_for_checks),
            &target_id,
            from,
        );
    }
    let from_visit_count = Some(render_visit_count(
        metadata_for_checks,
        &target_id,
        from,
        &current_state_raw,
        machine,
    ));
    let to_visit_count = updated_metadata
        .as_ref()
        .map(|meta| task_visit_count(Some(meta), &target_id, to))
        .filter(|count| *count > 0);

    if !origin.skip_source_outputs {
        ensure_state_outputs_exist_for_transition(
            &workspace_root,
            task_id_str,
            from,
            from_state_def,
            from_visit_count,
            machine,
            &settings,
        )?;
    }
    ensure_state_inputs_exist_for_transition(
        &workspace_root,
        task_id_str,
        to,
        to_state_def,
        to_visit_count,
        machine,
        &settings,
        &format!("Task {} cannot enter state {}.", task_id_str, to),
    )?;

    let rendered_to_state = format_task_state_value(to, to_visit_count, machine);
    let metadata_raw_updated = if task_file == metadata_file {
        let new_task_raw = rewrite_task_state(&task_raw, task_id_str, &rendered_to_state)?;
        if let Some(updated_metadata) = updated_metadata.as_ref() {
            rewrite_frontmatter(&new_task_raw, updated_metadata)?
        } else {
            new_task_raw
        }
    } else if let Some(updated_metadata) = updated_metadata.as_ref() {
        rewrite_frontmatter(&metadata_raw, updated_metadata)?
    } else {
        metadata_raw.clone()
    };

    let task_raw_updated = if task_file == metadata_file {
        None
    } else {
        Some(rewrite_task_state(&task_raw, task_id_str, &rendered_to_state)?)
    };

    // Atomic write(s): write to temp file in the same directory, then rename.
    write_file_atomic(metadata_file, &metadata_raw_updated)?;
    if let Some(ref task_raw_updated) = task_raw_updated {
        write_file_atomic(task_file, task_raw_updated)?;
    }

    // Execute on_enter callback after the state change (not model-looped).
    let triggered_by = origin.triggered_by.unwrap_or(if redirect_next_state.is_some() {
        "callback"
    } else {
        "user"
    });
    let on_enter_context_json = build_transition_context_json(
        plan_for_context.as_ref(),
        &callback_paths.plan_path,
        task_id_str,
        from,
        to,
        triggered_by,
        &transition_data,
        &callback_paths.working_dir,
    );
    let callback_ctx = CallbackContext {
        task_id: task_id_str,
        from_state: from,
        to_state: to,
        plan_path: &callback_paths.plan_path,
        callback_cwd: &callback_paths.working_dir,
        model: None,
        agent: None,
        context_json: Some(&on_enter_context_json),
    };
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_enter {
            let executor = ShellCallbackExecutor;
            let result = executor.execute(cb, &callback_ctx).map_err(|e| miette!("{e}"))?;
            if !result.success {
                // Spec §Example 8: on_enter failure rolls back the state
                // write to the original, then the error_handling policy
                // applies. We implement the rollback; policy execution is
                // a follow-up.
                let rollback_err = write_file_atomic(metadata_file, &metadata_raw).err();
                let task_rollback_err = if task_raw_updated.is_some() {
                    write_file_atomic(task_file, &task_raw).err()
                } else {
                    None
                };
                if let Some(task_handle) = &task_handle {
                    let _ = task_handle.unlock();
                }
                let _ = metadata_handle.unlock();
                let message =
                    result.error.clone().unwrap_or_else(|| "on_enter callback failed".to_string());
                if rollback_err.is_some() || task_rollback_err.is_some() {
                    return Err(miette!(
                        "on_enter callback '{}' failed ({message}); rollback also failed — plan file may be inconsistent",
                        cb.0
                    ));
                }
                return Err(miette!("on_enter callback '{}' failed: {message}", cb.0));
            }
        }
    }

    if let Some(task_handle) = task_handle {
        let _ = task_handle.unlock();
    }
    let _ = metadata_handle.unlock();
    Ok(to.to_string())
}

/// Extract the current state and node-policy inputs for a task from raw
/// markdown content.
///
/// Tries full-plan parsing first, falls back to workspace task-file parsing.
fn find_task_transition_info(
    raw: &str,
    file_path: &Path,
    target_id: &TaskId,
    task_id_str: &str,
) -> MietteResult<TransitionTaskInfo> {
    // Try full plan parse.
    if let Ok(rhei) = rhei_core::parse(raw) {
        if let Some(task) = find_task_by_id(&rhei.tasks, target_id) {
            return Ok(TransitionTaskInfo {
                current_state: task.state.as_str().to_string(),
                kind: task.kind.clone(),
                level: task.id.depth() as u8,
            });
        }
    }

    // Try workspace task-file parse.
    if let Ok(tasks) = rhei_core::parser::parse_workspace_tasks(raw) {
        if let Some(task) = find_task_by_id(&tasks, target_id) {
            return Ok(TransitionTaskInfo {
                current_state: task.state.as_str().to_string(),
                kind: task.kind.clone(),
                level: task.id.depth() as u8,
            });
        }
    }

    Err(miette!("task '{}' not found in {}", task_id_str, file_path.display()))
}

// ─── Agent Configuration ──────────────────────────────────────────────
