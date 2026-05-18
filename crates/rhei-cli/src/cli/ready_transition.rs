fn state_inputs_exist_for_ready_set(
    workspace_root: &Path,
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
        workspace_root,
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
) -> Vec<&'a rhei_core::ast::Task> {
    use std::collections::HashMap;

    // Build a map of task id → current state for dependency lookups.
    let state_map: HashMap<&TaskId, String> = rhei
        .tasks
        .iter()
        .map(|t| (&t.id, normalized_state_name(t.state.as_str(), machine)))
        .collect();

    let mut ready = Vec::new();

    for task in &rhei.tasks {
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

        if all_priors_done
            && state_inputs_exist_for_ready_set(
                workspace_root,
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
    find_ready_tasks(rhei, machine, workspace_root)
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .collect()
}

/// Find tasks that are ready to be claimed by `rhei next` in automatic mode.
///
/// A task is claimable when it is in the state machine's initial state, its
/// prerequisites are satisfied, and it has no `**Assignee:**` field (which
/// indicates it is already claimed by another agent). Already-claimed work
/// can still be inspected explicitly with `rhei next --task <id>`.
fn find_claimable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    workspace_root: &Path,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machine, workspace_root)
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .filter(|task| {
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).map(|def| def.initial).unwrap_or(false)
        })
        .collect()
}

/// Build an actionable error message for `rhei next` when no task can be
/// auto-claimed.
///
/// Distinguishes between three situations:
/// - every task is in a terminal state (nothing left to do),
/// - some task is ready but sitting in a non-initial state (mid-workflow —
///   the user needs `rhei transition` to pick an outgoing edge),
/// - every non-terminal task is blocked by unsatisfied prerequisites.
fn diagnose_no_claimable(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> String {
    use std::collections::HashMap;

    fn collect<'a>(task: &'a rhei_core::ast::Task, out: &mut Vec<&'a rhei_core::ast::Task>) {
        out.push(task);
        for c in &task.children {
            collect(c, out);
        }
    }
    let mut all = Vec::new();
    for t in &rhei.tasks {
        collect(t, &mut all);
    }

    if all.is_empty() {
        return "no tasks are ready to claim (plan has no tasks)".to_string();
    }

    let state_map: HashMap<&TaskId, String> =
        all.iter().map(|t| (&t.id, normalized_state_name(t.state.as_str(), machine))).collect();

    let non_terminal: Vec<&rhei_core::ast::Task> =
        all.iter().copied().filter(|t| !is_terminal_state(t.state.as_str(), machine)).collect();

    if non_terminal.is_empty() {
        return "no tasks are ready to claim: every task is already in a terminal state."
            .to_string();
    }

    let priors_satisfied = |task: &rhei_core::ast::Task| -> bool {
        task.prior.iter().all(|dep_id| {
            state_map.get(dep_id).map(|s| dependency_is_satisfied(s, machine)).unwrap_or(false)
        })
    };

    let ready_non_initial: Vec<&rhei_core::ast::Task> = non_terminal
        .iter()
        .copied()
        .filter(|t| {
            let s = normalized_state_name(t.state.as_str(), machine);
            let is_initial = machine.states.get(&s).map(|def| def.initial).unwrap_or(false);
            !is_initial && priors_satisfied(t)
        })
        .collect();

    if let Some(task) = ready_non_initial.first() {
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let outgoing: Vec<String> = machine
            .transitions()
            .iter()
            .filter(|rule| rule.from.0 == state_name || rule.from.0 == "*")
            .map(|rule| rule.to.0.clone())
            .collect();
        let outgoing =
            if outgoing.is_empty() { "(none declared)".to_string() } else { outgoing.join(", ") };
        return format!(
            "no tasks can be auto-claimed: Task {} is mid-workflow in state '{}'. \
             Pick one of its outgoing transitions [{}] with \
             `rhei transition <plan> --task {} --from {} --to <state>`.",
            task.id, state_name, outgoing, task.id, state_name
        );
    }

    let blocked: Vec<&rhei_core::ast::Task> =
        non_terminal.iter().copied().filter(|t| !priors_satisfied(t)).collect();
    if !blocked.is_empty() {
        let ids: Vec<String> = blocked.iter().take(3).map(|t| format!("Task {}", t.id)).collect();
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

fn try_auto_advance_task(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    current_state: &str,
    no_callbacks: bool,
    mut before_transition: Option<
        &mut dyn FnMut(&rhei_core::ast::Task, &str) -> MietteResult<()>,
    >,
) -> MietteResult<Option<String>> {
    // The spec splits agent exit into:
    //   (5) select the outgoing transition without applying it,
    //   (6) emit snapshots after selection / before application,
    //   (7) apply the selected transition.
    // See docs/functional-spec/rhei-run.spec.md § Execution Loop steps 5–7.
    // Step 6 is delegated to the snapshot module owned by impl-rhei-snapshots;
    // see `emit_snapshots_after_transition_selection` for the call site.
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(task_id_str);
    let Some(task) = loaded.rhei.tasks.iter().find(|t| t.id == target_id) else {
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

    execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        current_state,
        &to_state,
        no_callbacks,
    )?;

    Ok(Some(to_state))
}
