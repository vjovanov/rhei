fn state_inputs_exist_for_ready_set(
    workspace_root: &Path,
    artifact_root: &Path,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    state_name: &str,
) -> bool {
    let Some(state_def) = machine.states.get(state_name) else {
        return false;
    };
    if state_def.inputs.is_empty() {
        return true;
    }
    let settings = match load_merged_settings(workspace_root) {
        Ok(settings) => settings,
        Err(_) => return false,
    };
    let visit_count = Some(render_visit_count(
        rhei.metadata.as_ref(),
        &task.id,
        state_name,
        task.state.as_str(),
        machine,
    ));
    ensure_state_inputs_exist_for_transition(
        artifact_root,
        Some(task),
        &task.id.to_string(),
        state_name,
        state_def,
        visit_count,
        machine,
        &settings,
        "",
    )
    .is_ok()
}

/// Find tasks that are ready to advance: not in a terminal or gating state
/// and all prior dependencies are satisfied.
///
/// Returns task references in source order.
fn find_ready_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    workspace_root: &Path,
    task_roots: &std::collections::HashMap<String, std::path::PathBuf>,
    leaf_only: bool,
) -> Vec<&'a rhei_core::ast::Task> {
    use std::collections::HashMap;

    let mut all_tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all_tasks);

    // Build a map of every task node's state for dependency lookups. §FS-rhei-run.3
    let state_map: HashMap<&TaskId, String> = all_tasks
        .iter()
        .map(|t| (&t.id, normalized_state_name(t.state.as_str(), machine)))
        .collect();

    let mut ready = Vec::new();

    for task in all_tasks {
        if leaf_only && !task.children.is_empty() {
            continue;
        }
        let current_state = task.state.as_str();

        // Skip tasks already in a terminal or gating state.
        let normalized_state = normalized_state_name(current_state, machine);
        if is_terminal_state(current_state, machine)
            || machine.states.get(&normalized_state).map(|def| def.gating).unwrap_or(false)
        {
            continue;
        }

        if machine.states.get(&normalized_state).and_then(|def| def.poll.as_ref()).is_some()
            && poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &normalized_state)
                .is_some_and(|deadline| deadline > current_unix_secs())
        {
            continue;
        }

        // Check that all prior dependencies are satisfied.
        let all_priors_done = task.prior.iter().all(|dep_id| {
            state_map.get(dep_id).map(|s| dependency_is_satisfied(s, machine)).unwrap_or(false)
        });

        let task_id = task.id.to_string();
        // §AR-rhei-panta.5: input artifacts resolve from the owning rhei execution root.
        let artifact_root = task_roots.get(&task_id).map_or(workspace_root, |root| root.as_path());
        if all_priors_done
            && state_inputs_exist_for_ready_set(
                workspace_root,
                artifact_root,
                rhei,
                machine,
                task,
                &normalized_state,
            )
        {
            ready.push(task);
        }
    }

    ready
}

/// Find tasks that `rhei run` may schedule autonomously.
///
/// This keeps the broad readiness semantics used by the run loop, but skips
/// tasks that already carry an assignee so a manual claim cannot be stolen by
/// the orchestrator.
fn find_runnable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    workspace_root: &Path,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machine, workspace_root, &std::collections::HashMap::new(), false)
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .collect()
}

/// Find tasks that are ready to be claimed by `rhei next` in automatic mode.
///
/// A task is claimable when it is in the state machine's initial state, its
/// prerequisites are satisfied, and it has no `**Assignee:**` field (which
/// indicates it is already claimed by another agent).
fn find_claimable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    workspace_root: &Path,
    task_roots: &std::collections::HashMap<String, std::path::PathBuf>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machine, workspace_root, task_roots, true)
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .filter(|task| {
            let state = normalized_state_name(task.state.as_str(), machine);
            task_is_in_initial_state(task, &state, machine)
        })
        .collect()
}

fn task_is_in_initial_state(
    task: &rhei_core::ast::Task,
    normalized_state: &str,
    machine: &rhei_validator::StateMachine,
) -> bool {
    machine
        .profile_for_node(task.kind.as_str(), task.profile_level())
        .map(|profile| profile.initial == normalized_state)
        .unwrap_or_else(|| machine.states.get(normalized_state).map(|def| def.initial).unwrap_or(false))
}

fn collect_plan_tasks<'a>(
    tasks: &'a [rhei_core::ast::Task],
    out: &mut Vec<&'a rhei_core::ast::Task>,
) {
    for task in tasks {
        out.push(task);
        collect_plan_tasks(&task.children, out);
    }
}

fn plan_state_map<'a>(
    tasks: &[&'a rhei_core::ast::Task],
    machine: &rhei_validator::StateMachine,
) -> std::collections::HashMap<&'a TaskId, String> {
    tasks
        .iter()
        .map(|task| (&task.id, normalized_state_name(task.state.as_str(), machine)))
        .collect()
}

fn first_blocking_prior(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machine: &rhei_validator::StateMachine,
) -> Option<String> {
    task.prior.iter().find_map(|dep_id| match state_map.get(dep_id) {
        Some(state) if !dependency_is_satisfied(state, machine) => {
            Some(format!("Task {} ({})", dep_id, state))
        }
        None => Some(format!("Task {} (missing)", dep_id)),
        _ => None,
    })
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'_'
                | b'-'
                | b'.'
                | b'/'
                | b':'
                | b'@'
                | b'%'
                | b'+'
                | b'='
                | b','
        )
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn transition_command_lines(
    task: &rhei_core::ast::Task,
    state_name: &str,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    plan_arg: &str,
    state_machine_path: Option<&Path>,
) -> Vec<String> {
    let state_machine_arg = state_machine_path
        .map(|path| format!(" --state-machine={}", shell_quote(&path.display().to_string())))
        .unwrap_or_default();
    let from_arg = shell_quote(state_name);
    machine
        .transitions()
        .iter()
        .filter(|rule| rule.from.0 == state_name || rule.from.0 == "*")
        .filter(|rule| {
            task_profile_allows_state(
                machine,
                task.kind.as_str(),
                task.profile_level(),
                &rule.to.0,
            )
        })
        .filter(|rule| {
            transition_rule_is_applicable(
                rule,
                machine,
                metadata,
                &task.id,
                state_name,
                task.state.as_str(),
            )
            .unwrap_or(false)
        })
        .map(|rule| {
            let to_arg = shell_quote(&rule.to.0);
            format!(
                "  rhei{} transition {} --task {} --from={} --to={}",
                state_machine_arg, plan_arg, task.id, from_arg, to_arg
            )
        })
        .collect()
}

/// Build an actionable error message for `rhei next` when no task can be
/// auto-claimed.
fn diagnose_no_claimable(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
) -> String {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);

    if all.is_empty() {
        return "no tasks are ready to claim (plan has no tasks)".to_string();
    }

    let state_map = plan_state_map(&all, machine);

    let non_terminal: Vec<&rhei_core::ast::Task> =
        all.iter().copied().filter(|t| !is_terminal_state(t.state.as_str(), machine)).collect();

    if non_terminal.is_empty() {
        return format!(
            "Plan complete. All {} task(s) are in terminal states.",
            all.len()
        );
    }

    let leaf_tasks: Vec<&rhei_core::ast::Task> =
        all.iter().copied().filter(|task| task.children.is_empty()).collect();
    let non_terminal_rollups: Vec<&rhei_core::ast::Task> = all
        .iter()
        .copied()
        .filter(|task| !task.children.is_empty() && !is_terminal_state(task.state.as_str(), machine))
        .collect();
    if !leaf_tasks.is_empty()
        && leaf_tasks.iter().all(|task| is_terminal_state(task.state.as_str(), machine))
        && !non_terminal_rollups.is_empty()
    {
        let items: Vec<String> = non_terminal_rollups
            .iter()
            .take(3)
            .map(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                format!("Task {} ({})", task.id, state)
            })
            .collect();
        let suffix = if non_terminal_rollups.len() > 3 {
            format!(" (+{} more)", non_terminal_rollups.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "Leaf work complete. {} rollup task(s) can be completed after descendants are terminal: {}{}.",
            non_terminal_rollups.len(),
            items.join(", "),
            suffix
        );
    }

    let non_terminal_leaf_tasks: Vec<&rhei_core::ast::Task> = non_terminal
        .iter()
        .copied()
        .filter(|task| task.children.is_empty())
        .collect();

    let priors_satisfied = |task: &rhei_core::ast::Task| -> bool {
        task.prior.iter().all(|dep_id| {
            state_map.get(dep_id).map(|s| dependency_is_satisfied(s, machine)).unwrap_or(false)
        })
    };

    let gating_ready: Vec<&rhei_core::ast::Task> = non_terminal
        .iter()
        .copied()
        .filter(|task| task.children.is_empty())
        .filter(|task| {
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).map(|def| def.gating).unwrap_or(false)
                && priors_satisfied(task)
        })
        .collect();

    if !gating_ready.is_empty() {
        let items: Vec<String> = gating_ready
            .iter()
            .take(3)
            .map(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                format!("Task {} ({})", task.id, state)
            })
            .collect();
        let suffix = if gating_ready.len() > 3 {
            format!(" (+{} more)", gating_ready.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "Blocked: {} task(s) waiting on human action: {}{}.",
            gating_ready.len(),
            items.join(", "),
            suffix
        );
    }

    let assigned_ready: Vec<&rhei_core::ast::Task> = non_terminal_leaf_tasks
        .iter()
        .copied()
        .filter(|t| {
            let s = normalized_state_name(t.state.as_str(), machine);
            let gating = machine.states.get(&s).map(|def| def.gating).unwrap_or(false);
            !gating && t.assignee.is_some() && priors_satisfied(t)
        })
        .collect();

    if !assigned_ready.is_empty() {
        let items: Vec<String> = assigned_ready
            .iter()
            .take(3)
            .map(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                let assignee = task.assignee.as_deref().unwrap_or("unknown");
                format!("Task {} ({}, assignee {})", task.id, state, assignee)
            })
            .collect();
        let suffix = if assigned_ready.len() > 3 {
            format!(" (+{} more)", assigned_ready.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "No tasks available to claim. {} task(s) are currently in progress: {}{}.",
            assigned_ready.len(),
            items.join(", "),
            suffix
        );
    }

    let ready_non_initial: Vec<&rhei_core::ast::Task> = non_terminal_leaf_tasks
        .iter()
        .copied()
        .filter(|t| {
            let s = normalized_state_name(t.state.as_str(), machine);
            let gating = machine.states.get(&s).map(|def| def.gating).unwrap_or(false);
            !gating && !task_is_in_initial_state(t, &s, machine) && priors_satisfied(t)
        })
        .collect();

    if let Some(task) = ready_non_initial.first() {
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let plan_arg = shell_quote(&plan_path.display().to_string());
        let normalized_metadata = ensure_current_state_visit_count(
            rhei.metadata.as_ref(),
            &task.id,
            &state_name,
            task.state.as_str(),
            machine,
        );
        let metadata_for_checks = normalized_metadata.as_ref().or(rhei.metadata.as_ref());
        let commands = transition_command_lines(
            task,
            &state_name,
            machine,
            metadata_for_checks,
            &plan_arg,
            state_machine_path,
        );
        let guidance = if commands.is_empty() {
            "No outgoing transitions are currently applicable for this state.".to_string()
        } else {
            format!("Available transitions:\n{}", commands.join("\n"))
        };
        return format!(
            "No tasks can be auto-claimed: Task {} is mid-workflow in state '{}'. \
             Pick one of its outgoing transitions explicitly.\n{}",
            task.id, state_name, guidance
        );
    }

    let blocked: Vec<&rhei_core::ast::Task> =
        non_terminal_leaf_tasks.iter().copied().filter(|t| !priors_satisfied(t)).collect();
    if !blocked.is_empty() {
        let ids: Vec<String> = blocked
            .iter()
            .take(3)
            .map(|task| {
                if let Some(prior) = first_blocking_prior(task, &state_map, machine) {
                    format!("Task {} waiting on {}", task.id, prior)
                } else {
                    format!("Task {}", task.id)
                }
            })
            .collect();
        let suffix = if blocked.len() > 3 {
            format!(" (+{} more)", blocked.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "no tasks are ready to claim: {} blocked by incomplete prerequisites{}.",
            ids.join(", "),
            suffix
        );
    }

    // Fallback: we found non-terminal tasks with priors satisfied but no
    // other category matched. Keep the legacy phrasing for this edge case.
    "no tasks are ready to claim".to_string()
}

/// Check whether a state is terminal (final) in the state machine.
fn is_terminal_state(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    let normalized = normalized_state_name(state, machine);
    machine.states.get(&normalized).map(|def| def.terminal).unwrap_or(false)
}

fn state_declares_autonomous_execution(def: &rhei_validator::StateDef) -> bool {
    def.program.is_some()
        || def.agent.is_some()
        || def.model.is_some()
        || def.target.is_some()
        || !def.all_models.is_empty()
        || !def.all_targets.is_empty()
}

fn initial_state_has_non_terminal_forward_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<bool> {
    let Some(to_state) = find_next_transition(task, rhei, machine)? else {
        return Ok(false);
    };
    Ok(!machine.states.get(&to_state).map(|def| def.terminal).unwrap_or(false))
}

fn manual_initial_terminal_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<Option<String>> {
    // §FS-rhei-run.3: default manual-only tasks must not be callback-completed by `rhei run`.
    if !is_builtin_simple_manual_machine(machine) {
        return Ok(None);
    }
    let current_state = normalized_state_name(task.state.as_str(), machine);
    if !task_is_in_initial_state(task, &current_state, machine) {
        return Ok(None);
    }
    let Some(state_def) = machine.states.get(&current_state) else {
        return Ok(None);
    };
    if state_declares_autonomous_execution(state_def) {
        return Ok(None);
    }
    let Some(to_state) = find_next_transition(task, rhei, machine)? else {
        return Ok(None);
    };
    if machine.states.get(&to_state).map(|def| def.terminal).unwrap_or(false) {
        Ok(Some(to_state))
    } else {
        Ok(None)
    }
}

fn is_builtin_simple_manual_machine(machine: &rhei_validator::StateMachine) -> bool {
    machine.name == "rhei"
        && machine.states.len() == 2
        && machine.states.contains_key("pending")
        && machine.states.get("completed").map(|def| def.terminal).unwrap_or(false)
        && machine
            .transitions()
            .iter()
            .filter(|rule| rule.from.0 == "pending" && rule.to.0 == "completed")
            .count()
            == 1
}

/// Find the next forward transition from a given state.
///
/// Prefers exact `from` matches over wildcard (`*`) rules, and skips
/// transitions to terminal states via wildcards (those are escape hatches
/// like cancellation, not forward progress).
fn find_next_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<Option<String>> {
    let current_state = normalized_state_name(task.state.as_str(), machine);

    // First, look for an exact from-state match.
    for rule in machine.transitions() {
        if rule.from.0 == current_state
            && task_profile_allows_state(
                machine,
                task.kind.as_str(),
                task.profile_level(),
                &rule.to.0,
            )
            && transition_rule_is_applicable(
                rule,
                machine,
                rhei.metadata.as_ref(),
                &task.id,
                &current_state,
                task.state.as_str(),
            )?
        {
            return Ok(Some(rule.to.0.clone()));
        }
    }

    // Fall back to wildcard, but only to non-terminal states (forward progress).
    for rule in machine.transitions() {
        if rule.from.0 == "*" {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if !is_terminal
                && task_profile_allows_state(
                    machine,
                    task.kind.as_str(),
                    task.profile_level(),
                    &rule.to.0,
                )
                && transition_rule_is_applicable(
                    rule,
                    machine,
                    rhei.metadata.as_ref(),
                    &task.id,
                    &current_state,
                    task.state.as_str(),
                )?
            {
                return Ok(Some(rule.to.0.clone()));
            }
        }
    }

    Ok(None)
}

type BeforeTransitionCallback<'a> =
    &'a mut dyn FnMut(&rhei_core::ast::Task, &str) -> MietteResult<()>;

fn try_auto_advance_task(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    current_state: &str,
    no_callbacks: bool,
    mut before_transition: Option<BeforeTransitionCallback<'_>>,
) -> MietteResult<Option<String>> {
    // The spec splits agent exit into:
    //   (5) select the outgoing transition without applying it,
    //   (6) emit snapshots after selection / before application,
    //   (7) apply the selected transition.
    // Step 6 is delegated to the snapshot module owned by impl-rhei-snapshots;
    // see `emit_snapshots_after_transition_selection` for the call site.

    // §FS-rhei-run.3: Select, emit, then apply transitions.
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(task_id_str);
    let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) else {
        return Ok(None);
    };

    // Step 5: select the outgoing transition.
    let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? else {
        if machine.states.get(current_state).and_then(|def| def.poll.as_ref()).is_some()
            && task_visit_count(loaded.rhei.metadata.as_ref(), &task.id, current_state)
                >= machine
                    .states
                    .get(current_state)
                    .and_then(|def| def.poll.as_ref())
                    .map(|poll| u64::from(poll.max_attempts))
                    .unwrap_or(u64::MAX)
        {
            return Err(miette!(
                "polling exhausted with no matching non-self-loop transition for Task {} in state '{}'",
                task_id_str,
                current_state
            ));
        }
        return Ok(None);
    };

    if record_poll_self_loop_if_needed(
        input,
        loaded.rhei.metadata.as_ref(),
        machine,
        task,
        current_state,
        &to_state,
    )? {
        return Ok(Some(to_state));
    }

    // Step 6: emit auto- and named-snapshots for this state exit, before the
    // transition is applied. This is a no-op until impl-rhei-snapshots wires
    // the snapshot module in; the call site here pins the spec-mandated
    // ordering ("after transition selection and before the transition is
    // applied") so future wiring does not have to relitigate it.
    if let Some(before_transition) = before_transition.as_mut() {
        before_transition(task, &to_state)?;
    }
    emit_snapshots_after_transition_selection(machine, task, current_state, &to_state);

    // Step 7: apply the selected transition.
    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };

    let effective_to = execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        current_state,
        &to_state,
        no_callbacks,
    )?;
    append_transition_audit_entry(input, &task_file, task_id_str, current_state, &effective_to)?;

    Ok(Some(effective_to))
}
