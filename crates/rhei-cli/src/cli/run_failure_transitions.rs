
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
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    kind: ToolingKind,
    unavailable: &[String],
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    // The failing ticket's own machine and callback base fire the transition.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
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
    let route = loaded.task_route(task_id_str, input);
    match execute_system_tooling_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from_state,
        &rule.to.0,
        kind,
        unavailable,
        no_callbacks,
    ) {
        Ok(effective_to) => {
            diag_info!(
                "  Tooling-unavailable transition: Task {} '{}' -> '{}' ({} unavailable: {})",
                task_id_str,
                from_state,
                effective_to,
                kind.as_str(),
                unavailable.join(", ")
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            diag_warn!(
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
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    timeout_secs: Option<u64>,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    let machine = machines.for_task_str(task_id_str);
    let Some(to_state) = find_timeout_transition(machine, from_state) else {
        return TimeoutTransitionOutcome::NoRule;
    };
    fire_selected_timeout_transition(
        input,
        machines,
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
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    timeout_secs: Option<u64>,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    // The failing ticket's own machine and callback base fire the transition.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let route = loaded.task_route(task_id_str, input);
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
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from_state,
        to_state,
        &timeout_label,
        no_callbacks,
    ) {
        Ok(effective_to) => {
            diag_info!(
                "  Timeout transition: Task {} '{}' -> '{}' (timeout {})",
                task_id_str, from_state, effective_to, timeout_label
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            diag_warn!(
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
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    exit_code: i32,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    // The failing ticket's own machine and callback base fire the transition.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let route = loaded.task_route(task_id_str, input);
    match execute_system_program_exit_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from_state,
        to_state,
        exit_code,
        no_callbacks,
    ) {
        Ok(effective_to) => {
            diag_info!(
                "  Error transition: Task {} '{}' -> '{}' (exit {})",
                task_id_str, from_state, effective_to, exit_code
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            diag_warn!(
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

fn format_dry_run_transition(
    task_id: &str,
    from: &str,
    to: &str,
    machine: &rhei_validator::StateMachine,
) -> String {
    // A supervisor's self-loop is the release edge, and rendered bare it reads
    // as a no-op — the one line in a dry run that decides whether the subtree
    // beneath it moves. §FS-rhei-supervision.3.1
    let release = from == to && execute_on_of(machine, &normalized_state_name(from, machine)).is_some();
    let suffix = if release { " (release)" } else { "" };
    format!("would transition: Task {task_id}  {from} -> {to}{suffix}")
}

/// A dry run reports the manual-only condition instead of aborting on the
/// first task that hits it, so one invocation lists every blocked task
/// alongside the transitions that would run. §FS-rhei-run.4
fn format_dry_run_manual_only(task_id: &str, from: &str, to: &str) -> String {
    format!(
        "manual-only: Task {task_id}  {from} -> {to} \
         (claim with `rhei next`, finish with `rhei complete`)"
    )
}

/// The error a dry run ends with once it has reported every manual-only task.
///
/// The individual tasks were already streamed as `manual-only:` lines above,
/// so this only carries the count and the fix.
fn manual_only_dry_run_error(reported: &[String]) -> miette::Report {
    miette!(
help = "claim one with: rhei next <plan>",

        "{} task(s) reported above are in a manual-only initial state and cannot be advanced \
         by `rhei run`. Claim each with `rhei next`, do the work, then finish with \
         `rhei complete`.",
        reported.len()
    )
}

/// The error a dry run ends with when it found nothing to schedule and the
/// remaining tickets need a human.
///
/// The per-ticket causes were streamed above. The exit status matters as much
/// as the lines: a dry run that reported success while `rhei run` on the same
/// state halts is not a prediction, and the difference showed up as a wedged
/// queue nobody was warned about.
// §FS-rhei-run.4
fn dry_run_halt_error() -> miette::Report {
    miette!(
help = nothing_claimable_help(),

        "`rhei run` would halt here: nothing is schedulable and the tickets reported above \
         need a human. This is the same outcome the real run reaches."
    )
}

fn format_state_counts(rhei: &rhei_core::ast::Rhei) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    for task in tasks {
        *counts.entry(task.state.as_str()).or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(state, count)| format!("{state}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn total_task_count(rhei: &rhei_core::ast::Rhei) -> usize {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks.len()
}

fn terminal_task_count(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> usize {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks
        .into_iter()
        .filter(|task| is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .count()
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
    // §FS-rhei-states.1.4: the reserved cancel name, in either spelling.
    !rhei_validator::is_cancelled_state_name(&normalized_state_name(state, machine))
        && is_terminal_state(state, machine)
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

/// Whether any in-scope task is still non-terminal. Drives the end-of-run halt
/// check: a narrowed run only answers for its own scope. §FS-rhei-panta.6.1
fn scoped_unfinished_task_exists(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> bool {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks.into_iter().any(|task| {
        task_in_rhei_scope(scope, &task.id.to_string())
            && !is_terminal_state(task.state.as_str(), machines.for_task(&task.id))
    })
}

/// One-line no-work summary for `rhei run`. Project-wide it keeps the legacy
/// phrasing; under `--rhei` it names the scope and the blocked in-scope
/// candidates, marking priors that sit outside the scope. §FS-rhei-panta.6.1
fn no_advancement_summary(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> String {
    if scope.is_none() {
        return "No tasks could be advanced.".to_string();
    }
    let mut project = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut project);
    let state_map = plan_state_map(&project, machines);
    let blocked: Vec<String> = project
        .iter()
        .copied()
        // A parent whose subtree is still open is held up by the subtree, not
        // by a prior; its descendants report for themselves.
        // §FS-rhei-plan-language.3
        .filter(|task| descendants_are_terminal(task, machines))
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .filter_map(|task| {
            first_blocking_prior(task, &state_map, machines, scope)
                .map(|prior| format!("Task {} waiting on {}", task.id, prior))
        })
        .collect();
    let detail = if blocked.is_empty() {
        String::new()
    } else {
        let suffix =
            if blocked.len() > 3 { format!(" (+{} more)", blocked.len() - 3) } else { String::new() };
        format!(
            ": {}{}",
            blocked.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
            suffix
        )
    };
    format!("No tasks could be advanced in the --rhei scope ({}){}.", scope_label(scope), detail)
}

fn earliest_pending_poll_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Option<u64> {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks
        .into_iter()
        // §FS-rhei-panta.6.1: a narrowed run never advances out-of-scope
        // tickets, so their poll deadlines must not keep it alive.
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter_map(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).and_then(|def| def.poll.as_ref())?;
            poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &state)
        })
        .filter(|deadline| *deadline > current_unix_secs())
        .min()
}

/// Whether any non-terminal task sits in a gating state — work the run cannot
/// advance without a human decision. Lets an interactive run stay alive so the
/// gate stays resolvable in the UI. §FS-rhei-run-tui.1.5.5
fn has_pending_human_gate(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> bool {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks.iter().any(|task| {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        machine
            .states
            .get(&state)
            .map(|def| def.gating && !def.terminal)
            .unwrap_or(false)
    })
}

fn should_wait_for_human_gate(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> bool {
    // The gate itself may sit in any rhei — the TUI shows the whole project —
    // but only in-scope work decides whether waiting can still bear fruit.
    has_pending_human_gate(rhei, machines)
        && remaining_work_is_only_gating_or_poll_blocked(rhei, machines, scope)
}

/// One "is this ticket deliberately waiting?" judgment over a whole plan.
///
/// A ticket is deliberately waiting rather than stuck when it is held open by
/// its own subtree, parked in a gating state, inside a poll backoff window, or
/// waiting on a `**Prior:**` that is itself deliberately waiting.
///
/// One judgment, reached two ways — the top-level scan applies it to every
/// in-scope ticket, and the prior walk applies it to every ticket it reaches.
/// Judging a prior by its own state alone was the first bug: a dependent whose
/// prior is a parent held open by a gated child saw a non-gating state with no
/// priors of its own and read the run as stalled. A parent answers with its
/// subtree, so the dependent inherits the gate.
///
/// Answers are memoized per ticket, and the cycle guard is a *separate*
/// on-stack set. Using one visited set for both was the second bug: a revisit
/// is neutral-true inside the descendant `.all()` and neutral-false inside the
/// prior `.any()`, so a parent over `[gate, sibling waiting on the gate]` read
/// as stuck while the same two children in the other order read as waiting.
// §FS-rhei-plan-language.3 §FS-rhei-run-tui.1.5.5
struct DeliberateWaitJudgment<'a> {
    rhei: &'a rhei_core::ast::Rhei,
    tasks: Vec<&'a rhei_core::ast::Task>,
    state_map: HashMap<&'a TaskId, String>,
    machines: &'a rhei_validator::MachineSet,
    /// Verdicts already reached, so a ticket answers the same whichever walk
    /// arrives at it.
    memo: HashMap<TaskId, bool>,
    /// Tickets on the current recursion path, for cycle detection only.
    stack: HashSet<TaskId>,
}

impl<'a> DeliberateWaitJudgment<'a> {
    fn new(
        rhei: &'a rhei_core::ast::Rhei,
        machines: &'a rhei_validator::MachineSet,
    ) -> Self {
        let mut tasks = Vec::new();
        collect_plan_tasks(&rhei.tasks, &mut tasks);
        let state_map: HashMap<&'a TaskId, String> = tasks
            .iter()
            .map(|task| {
                (&task.id, normalized_state_name(task.state.as_str(), machines.for_task(&task.id)))
            })
            .collect();
        Self { rhei, tasks, state_map, machines, memo: HashMap::new(), stack: HashSet::new() }
    }

    /// The verdict for `task`, or `None` when `task` is already on the current
    /// walk. A cycle contributes nothing to either combinator, so each caller
    /// substitutes its own neutral element: `true` for the descendant `.all()`,
    /// `false` for the prior `.any()`.
    fn judge(&mut self, task: &'a rhei_core::ast::Task) -> Option<bool> {
        if let Some(answer) = self.memo.get(&task.id) {
            return Some(*answer);
        }
        if !self.stack.insert(task.id.clone()) {
            return None;
        }
        let answer = self.compute(task);
        self.stack.remove(&task.id);
        self.memo.insert(task.id.clone(), answer);
        Some(answer)
    }

    fn compute(&mut self, task: &'a rhei_core::ast::Task) -> bool {
        // A parent is not workable until its subtree closes, so it is blocked
        // by exactly whatever blocks its open descendants, each judged by this
        // same walk. §FS-rhei-plan-language.3
        let open = open_descendant_tasks(task, self.machines);
        if !open.is_empty() {
            return open.iter().copied().all(|child| self.judge(child).unwrap_or(true));
        }
        let machine = self.machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        // A terminal gate is a decision already taken, not one still pending —
        // the same reading `has_pending_human_gate` uses.
        if machine.states.get(&state).map(|def| def.gating && !def.terminal).unwrap_or(false) {
            return true;
        }
        if poll_next_attempt_at(self.rhei.metadata.as_ref(), &task.id, &state)
            .is_some_and(|deadline| deadline > current_unix_secs())
        {
            return true;
        }
        for dep_id in &task.prior {
            let Some(dep_state) = self.state_map.get(dep_id).cloned() else {
                continue;
            };
            // The prior's own machine says whether it satisfies.
            // §FS-rhei-panta.6.1
            if dependency_is_satisfied(&dep_state, self.machines.for_task(dep_id)) {
                continue;
            }
            let Some(dep_task) = self.tasks.iter().copied().find(|c| &c.id == dep_id) else {
                continue;
            };
            if self.judge(dep_task) == Some(true) {
                return true;
            }
        }
        false
    }
}

fn remaining_work_is_only_gating_or_poll_blocked(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> bool {
    let mut judgment = DeliberateWaitJudgment::new(rhei, machines);
    let tasks = judgment.tasks.clone();
    tasks
        .into_iter()
        // §FS-rhei-panta.6.1: "remaining work" is in-scope work; priors below
        // still resolve project-wide.
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .all(|task| judgment.judge(task).unwrap_or(true))
}
