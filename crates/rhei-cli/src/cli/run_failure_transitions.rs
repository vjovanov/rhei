
/// Result of [`fire_timeout_transition`]. The caller uses this to decide
/// whether to count the task as advanced and whether to emit the
/// "no timeout transition is declared" warning required by timeout behavior.
// §FS-rhei-agents.7.3: Timeout transition outcome handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeoutTransitionOutcome {
    /// A matching timeout transition fired successfully.
    Fired,
    /// No timeout transition is declared from the current state.
    NoRule,
    /// A matching rule existed but execution failed; details have already
    /// been logged.
    Failed,
}

fn tooling_trigger_matches(value: &serde_yaml::Value, unavailable: &[String]) -> bool {
    match value {
        serde_yaml::Value::Bool(true) => true,
        serde_yaml::Value::Sequence(items) => items.iter().any(|item| {
            item.as_str().map(|id| unavailable.iter().any(|u| u == id)).unwrap_or(false)
        }),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_tooling_unavailable_transition(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from_state: &str,
    kind: ToolingKind,
    unavailable: &[String],
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    let matching_rule = machine.transitions.iter().find(|rule| {
        let trigger = match kind {
            ToolingKind::Mcp => rule.mcp_unavailable.as_ref(),
            ToolingKind::Skill => rule.skill_unavailable.as_ref(),
        };
        (rule.from.0 == from_state || rule.from.0 == "*")
            && trigger.map(|value| tooling_trigger_matches(value, unavailable)).unwrap_or(false)
    });
    let Some(rule) = matching_rule else {
        return TimeoutTransitionOutcome::NoRule;
    };

    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };
    match execute_system_tooling_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        from_state,
        &rule.to.0,
        kind,
        unavailable,
        no_callbacks,
    ) {
        Ok(()) => {
            println!(
                "  Tooling-unavailable transition: Task {} '{}' -> '{}' ({} unavailable: {})",
                task_id_str,
                from_state,
                rule.to.0,
                kind.as_str(),
                unavailable.join(", ")
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            eprintln!(
                "  warning: failed to fire tooling-unavailable transition for Task {}: {}",
                task_id_str, err
            );
            TimeoutTransitionOutcome::Failed
        }
    }
}

fn find_timeout_transition(
    machine: &rhei_validator::StateMachine,
    from_state: &str,
) -> Option<String> {
    machine
        .transitions
        .iter()
        .find(|rule| (rule.from.0 == from_state || rule.from.0 == "*") && rule.timeout.is_some())
        .map(|rule| rule.to.0.clone())
}

/// Try to fire a timeout transition for a task after an agent was killed by
/// the watchdog. Returns whether a rule existed and whether it fired.
///
/// Sets `triggeredBy: 'system'` and `transitionData.timeout = <duration>`
/// on the resulting transition context (the duration is the agent's
/// resolved timeout, when known), matching timeout callback behavior.
// §FS-rhei-agents.7.5: Timeout callback context payload.
fn fire_timeout_transition(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from_state: &str,
    timeout_secs: Option<u64>,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    let Some(to_state) = find_timeout_transition(machine, from_state) else {
        return TimeoutTransitionOutcome::NoRule;
    };
    fire_selected_timeout_transition(
        input,
        machine,
        callback_paths,
        task_id_str,
        from_state,
        &to_state,
        timeout_secs,
        no_callbacks,
    )
}

#[allow(clippy::too_many_arguments)]
fn fire_selected_timeout_transition(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    timeout_secs: Option<u64>,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };
    let timeout_label = timeout_secs
        .map(format_duration_human)
        .or_else(|| {
            machine
                .transitions
                .iter()
                .find(|rule| {
                    (rule.from.0 == from_state || rule.from.0 == "*")
                        && rule.timeout.is_some()
                        && rule.to.0 == to_state
                })
                .and_then(|rule| rule.timeout.clone())
        })
        .unwrap_or_default();
    match execute_system_timeout_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        from_state,
        to_state,
        &timeout_label,
        no_callbacks,
    ) {
        Ok(()) => {
            println!(
                "  Timeout transition: Task {} '{}' -> '{}' (timeout {})",
                task_id_str, from_state, to_state, timeout_label
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            eprintln!(
                "  warning: failed to fire timeout transition for Task {}: {}",
                task_id_str, err
            );
            TimeoutTransitionOutcome::Failed
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_agent_exit_transition(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    exit_code: i32,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };
    match execute_system_program_exit_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        from_state,
        to_state,
        exit_code,
        no_callbacks,
    ) {
        Ok(()) => {
            println!(
                "  Error transition: Task {} '{}' -> '{}' (exit {})",
                task_id_str, from_state, to_state, exit_code
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            eprintln!(
                "  warning: failed to fire error transition for Task {}: {}",
                task_id_str, err
            );
            TimeoutTransitionOutcome::Failed
        }
    }
}

fn format_task_label(task: &rhei_core::ast::Task) -> String {
    format!("Task {}: {}", task.id, task.title)
}

fn format_ready_tasks(tasks: &[&rhei_core::ast::Task]) -> String {
    tasks.iter().map(|task| format_task_label(task)).collect::<Vec<_>>().join(", ")
}

fn format_dry_run_transition(task_id: &str, from: &str, to: &str) -> String {
    format!("would transition: Task {task_id}  {from} -> {to}")
}

fn format_state_counts(rhei: &rhei_core::ast::Rhei) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    for task in &rhei.tasks {
        *counts.entry(task.state.as_str()).or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(state, count)| format!("{state}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn newly_discovered_tasks(
    task_ids_before: &BTreeSet<String>,
    tasks_after: &[rhei_core::ast::Task],
) -> Vec<String> {
    tasks_after
        .iter()
        .filter(|task| !task_ids_before.contains(&task.id.to_string()))
        .map(format_task_label)
        .collect()
}

/// Check whether a dependency state satisfies a prerequisite edge.
///
/// Terminal cancellation does not satisfy dependencies: a cancelled task should
/// not unblock downstream work.
fn dependency_is_satisfied(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    normalized_state_name(state, machine) != "cancelled" && is_terminal_state(state, machine)
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn yaml_value_to_epoch_secs(value: &YamlValue) -> Option<u64> {
    match value {
        YamlValue::Number(number) => number.as_u64(),
        YamlValue::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn poll_next_attempt_at(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<u64> {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key("pollNextAttemptAt")))
        .and_then(YamlValue::as_mapping)
        .and_then(|poll_map| poll_map.get(yaml_key(state_name)))
        .and_then(yaml_value_to_epoch_secs)
}

fn earliest_pending_poll_deadline(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> Option<u64> {
    rhei.tasks
        .iter()
        .filter_map(|task| {
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).and_then(|def| def.poll.as_ref())?;
            poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &state)
        })
        .filter(|deadline| *deadline > current_unix_secs())
        .min()
}

fn remaining_work_is_only_gating_or_poll_blocked(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> bool {
    let state_map: HashMap<&TaskId, String> = rhei
        .tasks
        .iter()
        .map(|task| (&task.id, normalized_state_name(task.state.as_str(), machine)))
        .collect();

    fn blocked_by_gate<'a>(
        task: &'a rhei_core::ast::Task,
        tasks: &'a [rhei_core::ast::Task],
        state_map: &HashMap<&'a TaskId, String>,
        machine: &rhei_validator::StateMachine,
        seen: &mut HashSet<TaskId>,
    ) -> bool {
        task.prior.iter().any(|dep_id| {
            if !seen.insert(dep_id.clone()) {
                return false;
            }
            let Some(dep_state) = state_map.get(dep_id) else {
                return false;
            };
            let dep_is_gate = machine.states.get(dep_state).map(|def| def.gating).unwrap_or(false);
            if dep_is_gate {
                return true;
            }
            if dependency_is_satisfied(dep_state, machine) {
                return false;
            }
            tasks
                .iter()
                .find(|candidate| &candidate.id == dep_id)
                .is_some_and(|dep_task| blocked_by_gate(dep_task, tasks, state_map, machine, seen))
        })
    }

    rhei.tasks.iter().filter(|task| !is_terminal_state(task.state.as_str(), machine)).all(|task| {
        let state = normalized_state_name(task.state.as_str(), machine);
        if machine.states.get(&state).map(|def| def.gating).unwrap_or(false) {
            return true;
        }
        if poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &state)
            .is_some_and(|deadline| deadline > current_unix_secs())
        {
            return true;
        }
        blocked_by_gate(task, &rhei.tasks, &state_map, machine, &mut HashSet::new())
    })
}
