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

/// Every descendant of `task` that is not yet terminal, in preorder.
// §DA-per-rhei-state-machines: each node is judged under the machine its own
// id resolves to, so a mixed-machine project cannot misread a child's state.
fn open_descendant_tasks<'a>(
    task: &'a rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> Vec<&'a rhei_core::ast::Task> {
    fn recurse<'a>(
        task: &'a rhei_core::ast::Task,
        machines: &rhei_validator::MachineSet,
        out: &mut Vec<&'a rhei_core::ast::Task>,
    ) {
        for child in &task.children {
            if !is_terminal_state(child.state.as_str(), machines.for_task(&child.id)) {
                out.push(child);
            }
            recurse(child, machines, out);
        }
    }
    let mut out = Vec::new();
    recurse(task, machines, &mut out);
    out
}

/// Whether `task` has any non-terminal descendant.
///
/// The eligibility rule below only asks whether the subtree is closed, and it
/// is asked for every node on every scheduling pass, so this stops at the first
/// open node instead of materializing the whole list.
// §DA-per-rhei-state-machines: each node is judged under the machine its own
// id resolves to, so a mixed-machine project cannot misread a child's state.
fn any_open_descendant(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> bool {
    task.children.iter().any(|child| {
        !is_terminal_state(child.state.as_str(), machines.for_task(&child.id))
            || any_open_descendant(child, machines)
    })
}

/// The one eligibility rule for non-leaf tasks, shared by `rhei next` and
/// `rhei run` instead of splitting on whether the node has children.
// §FS-rhei-plan-language.3 §FS-rhei-next.3: workable once the subtree is
// terminal; a leaf satisfies it trivially.
fn descendants_are_terminal(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> bool {
    !any_open_descendant(task, machines)
}

/// `Task <id> (<state>)` for each open descendant, capped at three with a
/// `(+N more)` tail — the shape every other ticket list in this module uses.
fn format_open_descendants(
    open: &[&rhei_core::ast::Task],
    machines: &rhei_validator::MachineSet,
) -> String {
    let items: Vec<String> = open
        .iter()
        .take(3)
        .map(|task| {
            format!(
                "Task {} ({})",
                task.id,
                normalized_state_name(task.state.as_str(), machines.for_task(&task.id))
            )
        })
        .collect();
    let suffix =
        if open.len() > 3 { format!(" (+{} more)", open.len() - 3) } else { String::new() };
    format!("{}{}", items.join(", "), suffix)
}

/// Find tasks that are ready to advance: not in a terminal or gating state,
/// with every descendant terminal, and all prior dependencies satisfied.
///
/// Returns task references in source order.
fn find_ready_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    workspace_root: &Path,
    task_roots: &std::collections::HashMap<String, std::path::PathBuf>,
) -> Vec<&'a rhei_core::ast::Task> {
    use std::collections::HashMap;

    let mut all_tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all_tasks);

    // Build a map of every task node's state for dependency lookups, each
    // normalized under its owning rhei's machine. §FS-rhei-run.3
    let state_map: HashMap<&TaskId, String> = all_tasks
        .iter()
        .map(|t| (&t.id, normalized_state_name(t.state.as_str(), machines.for_task(&t.id))))
        .collect();

    let mut ready = Vec::new();

    for task in all_tasks {
        // §FS-rhei-plan-language.3: a non-leaf task is workable only once its
        // subtree is terminal — the same rule for `rhei next` and `rhei run`,
        // so a parent is never worked beside its own child.
        if !descendants_are_terminal(task, machines) {
            continue;
        }
        let machine = machines.for_task(&task.id);
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

        // Check that all prior dependencies are satisfied — each judged under
        // the machine of the rhei that owns the prior. §FS-rhei-panta.6.1
        let all_priors_done = task.prior.iter().all(|dep_id| {
            state_map
                .get(dep_id)
                .map(|s| dependency_is_satisfied(s, machines.for_task(dep_id)))
                .unwrap_or(false)
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
/// This keeps the readiness semantics used by the run loop, but skips
/// tasks that already carry an assignee so a manual claim cannot be stolen by
/// the orchestrator.
fn find_runnable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    workspace_root: &Path,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machines, workspace_root, &std::collections::HashMap::new())
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .collect()
}

/// Ready tickets `rhei run` will not touch because someone already holds them.
/// The loop counts only what it can schedule, so a held ticket vanished from
/// the pass report and read as "not ready". §FS-rhei-run.3
fn find_held_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    workspace_root: &Path,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machines, workspace_root, &std::collections::HashMap::new())
        .into_iter()
        .filter(|task| task.assignee.is_some())
        .collect()
}

/// One line naming every held ticket and who holds it.
fn format_held_tasks(held: &[&rhei_core::ast::Task]) -> String {
    held.iter()
        .map(|task| {
            format!("Task {} (assignee {})", task.id, task.assignee.as_deref().unwrap_or("?"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Find tasks that are ready to be claimed by `rhei next` in automatic mode.
///
/// A task is claimable when every descendant of it is terminal, it is in the
/// state machine's initial state, its prerequisites are satisfied, and it has
/// no `**Assignee:**` field (already claimed by another agent).
// §FS-rhei-next.3
fn find_claimable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    workspace_root: &Path,
    task_roots: &std::collections::HashMap<String, std::path::PathBuf>,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machines, workspace_root, task_roots)
        .into_iter()
        .filter(|task| task.assignee.is_none())
        .filter(|task| {
            let machine = machines.for_task(&task.id);
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
    machines: &rhei_validator::MachineSet,
) -> std::collections::HashMap<&'a TaskId, String> {
    tasks
        .iter()
        .map(|task| {
            (&task.id, normalized_state_name(task.state.as_str(), machines.for_task(&task.id)))
        })
        .collect()
}

/// Every unsatisfied `**Prior:**` of `task` as `Task <id> (<state>)`. Judged
/// exactly as readiness judges it, so mutation commands and the scheduler
/// agree on what "blocked" means. §FS-rhei-panta.6.1
fn blocking_priors(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
) -> Vec<String> {
    task.prior
        .iter()
        .filter_map(|dep_id| match state_map.get(dep_id) {
            Some(state) if !dependency_is_satisfied(state, machines.for_task(dep_id)) => {
                Some(format!("Task {} ({})", dep_id, state))
            }
            None => Some(format!("Task {} (missing)", dep_id)),
            _ => None,
        })
        .collect()
}

fn first_blocking_prior(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Option<String> {
    task.prior.iter().find_map(|dep_id| match state_map.get(dep_id) {
        Some(state) if !dependency_is_satisfied(state, machines.for_task(dep_id)) => {
            // §FS-rhei-panta.6.1: `--rhei` narrows candidates, never prior
            // resolution, so name the prior that sits outside the scope
            // rather than leaving the operator to guess why nothing ran.
            let outside = if task_in_rhei_scope(scope, &dep_id.to_string()) {
                ""
            } else {
                ", outside the --rhei scope"
            };
            Some(format!("Task {} ({}{})", dep_id, state, outside))
        }
        None => Some(format!("Task {} (missing)", dep_id)),
        _ => None,
    })
}

/// Why one non-terminal ticket is not moving.
///
/// `rhei next` already tells a worker exactly which of these applies. Every
/// `rhei run` surface — the halt message, the dry
/// run, and the durable report's Attention table — collapsed all of them into
/// "stalled in non-terminal state <s>" with "inspect logs or mark the task
/// cancelled" as the advice: wrong for a claimed ticket, wrong for one waiting
/// on a prior, and pointing at logs a run that spawned nothing never wrote.
// §FS-rhei-run-report.3.1 §FS-rhei-run.4: one classification, every surface.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HaltCause {
    /// A non-leaf ticket whose own subtree is still open. It is a task in its
    /// own right, so it is reported — but the work is in the descendants.
    /// §FS-rhei-plan-language.3
    WaitingOnDescendants { open: String },
    /// A gating state deliberately waiting for a human decision.
    Gate,
    /// A live `**Assignee:**`; the scheduler never schedules a claimed ticket.
    Claimed { assignee: String },
    /// An unsatisfied `**Prior:**`, already formatted as `Task <id> (<state>)`.
    BlockedByPrior { prior: String },
    /// Manual-only initial state: `rhei run` must not advance it.
    ManualOnly { to: String },
    /// Non-terminal with no declared outgoing transition to take.
    NoTransition,
    /// None of the above — work was possible and the ticket is still here.
    Stalled,
}

impl HaltCause {
    /// The reason and the next action, for the report table and the halt
    /// diagnostics. Both name concrete commands wherever one exists.
    fn describe(&self, id: &str, state: &str) -> (String, String) {
        match self {
            HaltCause::WaitingOnDescendants { open } => (
                format!("waiting on open descendant {open}"),
                "finish the descendants; the parent is claimable once its subtree is terminal"
                    .to_string(),
            ),
            HaltCause::Gate => (
                "gating state awaiting review".to_string(),
                "transition manually when reviewed".to_string(),
            ),
            HaltCause::Claimed { assignee } => (
                format!("claimed by {assignee}"),
                format!(
                    "`rhei release {id}` to hand it back, or `rhei complete {id} --result …` \
                     to finish it"
                ),
            ),
            HaltCause::BlockedByPrior { prior } => (
                format!("waiting on {prior}"),
                "finish the prior first".to_string(),
            ),
            HaltCause::ManualOnly { to } => (
                format!("manual-only initial state '{state}' with terminal transition to '{to}'"),
                format!("`rhei next` to claim, do the work, then `rhei complete {id} --result …`"),
            ),
            HaltCause::NoTransition => (
                format!("no forward transition available from '{state}'"),
                "declare a transition out of this state, or cancel the ticket".to_string(),
            ),
            HaltCause::Stalled => (
                if state.is_empty() {
                    "no forward transition available".to_string()
                } else {
                    format!("stalled in non-terminal state {state}")
                },
                "inspect logs or mark the task cancelled".to_string(),
            ),
        }
    }

    /// Whether this cause is a deliberate pause rather than something wrong.
    /// A gate is the plan working as authored; the rest need a human to act.
    // §FS-rhei-run-report.3.1: a parent waiting on its own subtree is the
    // eligibility rule working — the descendants answer for themselves.
    fn is_deliberate_pause(&self) -> bool {
        matches!(self, HaltCause::Gate | HaltCause::WaitingOnDescendants { .. })
    }
}

/// Classify why a non-terminal ticket did not advance. `worked` marks a ticket
/// the run actually spawned work for, whose failure is the ordinary stalled
/// case rather than a scheduling one.
fn classify_halt(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    state_map: &std::collections::HashMap<&TaskId, String>,
    scope: &RheiScope,
    worked: bool,
) -> HaltCause {
    let machine = machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    // A parent is not schedulable at all until its subtree closes, so that
    // outranks anything about its own state. §FS-rhei-plan-language.3
    let open = open_descendant_tasks(task, machines);
    if !open.is_empty() {
        return HaltCause::WaitingOnDescendants { open: format_open_descendants(&open, machines) };
    }
    if machine.states.get(&state).map(|def| def.gating).unwrap_or(false) {
        return HaltCause::Gate;
    }
    // A claim outranks a prior: releasing it is the one action that unblocks
    // the scheduler, and a claimed ticket is skipped before priors are read.
    if let Some(assignee) = task.assignee.as_deref() {
        return HaltCause::Claimed { assignee: assignee.to_string() };
    }
    if let Some(prior) = first_blocking_prior(task, state_map, machines, scope) {
        return HaltCause::BlockedByPrior { prior };
    }
    if let Ok(Some(to)) = manual_initial_terminal_transition(task, rhei, machine) {
        return HaltCause::ManualOnly { to };
    }
    if worked {
        return HaltCause::Stalled;
    }
    match find_next_transition(task, rhei, machine) {
        Ok(None) => HaltCause::NoTransition,
        _ => HaltCause::Stalled,
    }
}

/// Every in-scope, non-terminal ticket with why it is not moving, in plan
/// order — the shared basis for the run's halt diagnostics and the report.
///
/// `worked` reports whether the run actually spawned an invocation for a
/// ticket; those failed at their work rather than at scheduling, so they keep
/// the generic stalled reading.
// §FS-rhei-run-report.3.1: non-leaf tickets are classified alongside leaves, so
// a parent nobody can advance is nameable as the reason a dependent is stuck.
fn classify_halted_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
    worked: &dyn Fn(&str) -> bool,
) -> Vec<(&'a rhei_core::ast::Task, HaltCause)> {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let state_map = plan_state_map(&all, machines);
    all.iter()
        .copied()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .map(|task| {
            let cause =
                classify_halt(task, rhei, machines, &state_map, scope, worked(&task.id.to_string()));
            (task, cause)
        })
        .collect()
}

/// One `Task <id> (<state>): <reason> — <next action>` line per halted ticket,
/// plus whether any of them needs a human to act — which is what makes a run,
/// real or dry, end non-zero. The caller emits the lines through its own run
/// journal.
// §FS-rhei-run.4
fn halted_task_report(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut needs_human = false;
    for (task, cause) in classify_halted_tasks(rhei, machines, scope, &|_| false) {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        let id = task.id.to_string();
        let (reason, next) = cause.describe(&id, &state);
        lines.push(format!("Task {id} ({state}): {reason} \u{2014} {next}"));
        if !cause.is_deliberate_pause() {
            needs_human = true;
        }
    }
    (lines, needs_human)
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
/// auto-claimed. Priors resolve project-wide even under `--rhei`, so only the
/// reported categories narrow — never the state map. §FS-rhei-panta.6.1
fn diagnose_no_claimable(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    scope: &RheiScope,
) -> String {
    let mut project = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut project);

    let state_map = plan_state_map(&project, machines);

    let all: Vec<&rhei_core::ast::Task> = project
        .iter()
        .copied()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .collect();

    let scope_suffix = match scope {
        Some(_) => format!(" in the --rhei scope ({})", scope_label(scope)),
        None => String::new(),
    };

    if all.is_empty() {
        return match scope {
            Some(_) => {
                format!("no tickets are ready to claim{scope_suffix} (no tickets in scope)")
            }
            // Match the vocabulary and the next step `rhei list` gives for the
            // same state; a bare "plan has no tasks" leaves a new user stuck.
            None if rhei_core::workspace::is_panta_project(plan_path) => {
                format!(
                    "no tickets are ready to claim — the project has none yet: {}",
                    add_a_rhei_hint()
                )
            }
            None => "no tickets are ready to claim — this rhei has none yet".to_string(),
        };
    }

    let non_terminal: Vec<&rhei_core::ast::Task> = all
        .iter()
        .copied()
        .filter(|t| !is_terminal_state(t.state.as_str(), machines.for_task(&t.id)))
        .collect();

    if non_terminal.is_empty() {
        return match scope {
            Some(_) => format!(
                "Scope complete. All {} task(s){scope_suffix} are in terminal states.",
                all.len()
            ),
            None => format!("Plan complete. All {} task(s) are in terminal states.", all.len()),
        };
    }

    // Every category below speaks about a task the caller can act on, so each
    // is computed over the *workable* set: leaves, plus non-leaf tasks whose
    // subtree is already terminal. A parent with open descendants is not work
    // anyone can be handed; it gets its own category last, so it can still
    // explain a stuck plan instead of falling through to a bare "nothing is
    // ready". The retired "Leaf work complete. <N> rollup task(s) …" message
    // existed only because a parent could never be claimed at all — under the
    // eligibility rule it simply becomes the next claimable ticket the moment
    // its own children finish.

    // §FS-rhei-next.5
    let non_terminal_workable: Vec<&rhei_core::ast::Task> = non_terminal
        .iter()
        .copied()
        .filter(|task| descendants_are_terminal(task, machines))
        .collect();

    let priors_satisfied = |task: &rhei_core::ast::Task| -> bool {
        task.prior.iter().all(|dep_id| {
            state_map
                .get(dep_id)
                .map(|s| dependency_is_satisfied(s, machines.for_task(dep_id)))
                .unwrap_or(false)
        })
    };

    let gating_ready: Vec<&rhei_core::ast::Task> = non_terminal_workable
        .iter()
        .copied()
        .filter(|task| {
            let machine = machines.for_task(&task.id);
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
                let state =
                    normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
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

    let assigned_ready: Vec<&rhei_core::ast::Task> = non_terminal_workable
        .iter()
        .copied()
        .filter(|t| {
            let machine = machines.for_task(&t.id);
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
                let state =
                    normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
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
            "No tasks available to claim{}. {} task(s) are currently in progress: {}{}.",
            scope_suffix,
            assigned_ready.len(),
            items.join(", "),
            suffix
        );
    }

    let ready_non_initial: Vec<&rhei_core::ast::Task> = non_terminal_workable
        .iter()
        .copied()
        .filter(|t| {
            let machine = machines.for_task(&t.id);
            let s = normalized_state_name(t.state.as_str(), machine);
            let gating = machine.states.get(&s).map(|def| def.gating).unwrap_or(false);
            !gating && !task_is_in_initial_state(t, &s, machine) && priors_satisfied(t)
        })
        .collect();

    if let Some(task) = ready_non_initial.first() {
        let machine = machines.for_task(&task.id);
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
        non_terminal_workable.iter().copied().filter(|t| !priors_satisfied(t)).collect();
    if !blocked.is_empty() {
        let ids: Vec<String> = blocked
            .iter()
            .take(3)
            .map(|task| {
                if let Some(prior) = first_blocking_prior(task, &state_map, machines, scope) {
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
            "no tickets are ready to claim{}: {} ticket(s) blocked by incomplete prerequisites: {}{}.",
            scope_suffix,
            blocked.len(),
            ids.join(", "),
            suffix
        );
    }

    // No branch for "only parents are left": a parent with an open descendant
    // always has a non-terminal leaf under it, and that leaf is workable, so
    // the categories above speak for the subtree. A worker whose dependent is
    // blocked on an unclaimed parent reads it in the prerequisite branch,
    // which names the parent and its state like any other prior.

    // Fallback: we found non-terminal tasks with priors satisfied but no
    // other category matched. Keep the legacy phrasing for this edge case.
    format!("no tickets are ready to claim{scope_suffix}")
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
    machines: &ExecutionMachines,
    task_id_str: &str,
    current_state: &str,
    no_callbacks: bool,
    mut before_transition: Option<BeforeTransitionCallback<'_>>,
) -> MietteResult<Option<String>> {
    // The advancing ticket's own machine and callback base govern it.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
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
                help = "the poll state ran out of attempts without a transition becoming applicable. Raise its `poll.max_attempts`, or fix the condition the poll waits on: rhei states",
                "polling exhausted with no matching non-self-loop transition for Task {} in state '{}'",
                task_id_str,
                current_state
            ));
        }
        return Ok(None);
    };

    if record_poll_self_loop_if_needed(
        &loaded,
        input,
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

    // Step 7: apply the selected transition, routed to the owning rhei.
    let route = loaded.task_route(task_id_str, input);

    let effective_to = execute_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        current_state,
        &to_state,
        no_callbacks,
    )?;
    append_transition_audit_entry(&route, machine, task_id_str, current_state, &effective_to)?;

    Ok(Some(effective_to))
}
