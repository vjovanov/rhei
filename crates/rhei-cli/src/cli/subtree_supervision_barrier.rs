// The barrier itself: what one applied transition changes about supervision —
// the moving task's own phase, and the checkpoint its nearest supervising
// ancestor is owed — and which tasks the barrier admits to the ready set.
//
// Its own part because this reads the plan tree and decides, while the metadata
// block next door only stores what it decided.

// §AR-source-file-size.3 §FS-rhei-supervision.2 §FS-rhei-supervision.3

// ---------------------------------------------------------------------------
// The shared transition path
// ---------------------------------------------------------------------------

/// The chain from `target`'s parent up to its root, nearest ancestor first.
fn ancestor_chain<'a>(
    tasks: &'a [rhei_core::ast::Task],
    target: &TaskId,
) -> Vec<&'a rhei_core::ast::Task> {
    fn walk<'a>(
        tasks: &'a [rhei_core::ast::Task],
        target: &TaskId,
        stack: &mut Vec<&'a rhei_core::ast::Task>,
    ) -> bool {
        for task in tasks {
            if &task.id == target {
                return true;
            }
            stack.push(task);
            if walk(&task.children, target, stack) {
                return true;
            }
            stack.pop();
        }
        false
    }
    let mut stack = Vec::new();
    if walk(tasks, target, &mut stack) {
        stack.reverse();
        return stack;
    }
    Vec::new()
}

/// Whether a supervisor is working right now, so a move under it is its own
/// doing rather than news for it.
///
/// Two facts answer it, and both are visible from a `rhei transition` that the
/// supervisor's own subprocess or worker issued: the `**Assignee:**` a manual
/// claim writes, and the `RHEI_TASK_ID` every invocation `rhei run` spawns
/// carries. A descendant's own worker carries its *own* id, so it cannot be
/// mistaken for its supervisor.
// §FS-rhei-supervision.2.1 §FS-rhei-supervision.3.2
fn supervisor_is_in_flight(supervisor: &rhei_core::ast::Task, local_id: &str) -> bool {
    if supervisor.assignee.is_some() {
        return true;
    }
    let matches = |var: &str, want: &str| {
        std::env::var(var).is_ok_and(|value| value == want)
    };
    matches("RHEI_TASK_ID", &supervisor.id.to_string()) || matches("RHEI_TASK_ID_LOCAL", local_id)
}

/// One applied transition, as supervision reads it.
struct SupervisionTransition<'a> {
    machine: &'a rhei_validator::StateMachine,
    /// The transitioning task and its ancestors, as re-read from the plan.
    task: &'a rhei_core::ast::Task,
    ancestors: &'a [rhei_core::ast::Task],
    /// `metadata.tasks.<id>` key of the transitioning task.
    metadata_key: &'a TaskId,
    /// What turns a rhei-local id into a metadata key. Empty outside the basin,
    /// whose metadata shares the project manifest under qualified ids.
    metadata_prefix: &'a str,
    /// The transitioning task's rhei-local id: what a checkpoint records.
    local_id: &'a str,
    from: &'a str,
    /// The *effective* target, after any callback redirect.
    to: &'a str,
    /// Visit number of `to` after the move.
    to_visit: u64,
}

impl SupervisionTransition<'_> {
    fn to_is_terminal(&self) -> bool {
        self.machine.states.get(self.to).map(|def| def.terminal).unwrap_or(false)
    }

    /// The checkpoint this move produces for the nearest supervising ancestor,
    /// with the ancestor it is owed to. `None` when the move is not news.
    // §FS-rhei-supervision.2.1 §FS-rhei-supervision.2.2
    fn checkpoint(&self) -> Option<(&rhei_core::ast::Task, SupervisionCheckpoint)> {
        if self.from == self.to {
            // A poll attempt is a retry, and a supervisor's own release edge is
            // the supervisor waiting: neither is the subtree progressing.
            let from_def = self.machine.states.get(self.from);
            if from_def.map(|def| def.poll.is_some()).unwrap_or(false)
                || from_def.and_then(|def| def.supervise_kind()).is_some()
            {
                return None;
            }
        }
        // §FS-rhei-supervision.2.2: exactly one task hears about it.
        let supervisor = self
            .ancestors
            .iter()
            .find(|ancestor| task_is_supervising(ancestor, self.machine))?;
        let kind = supervise_kind_of(
            self.machine,
            &normalized_state_name(supervisor.state.as_str(), self.machine),
        )?;
        if kind == rhei_validator::SuperviseKind::Task && !self.to_is_terminal() {
            return None;
        }
        if supervisor_is_in_flight(supervisor, &supervisor_local_id(supervisor, self.local_id, self.task)) {
            return None;
        }
        Some((
            supervisor,
            SupervisionCheckpoint {
                task: self.local_id.to_string(),
                from: self.from.to_string(),
                to: self.to.to_string(),
                visit: self.to_visit.max(1),
            },
        ))
    }
}

/// The ancestor's id as its own task file spells it.
///
/// The transitioning task's rhei-local id is the qualified one minus a fixed
/// prefix, and every ancestor shares that prefix, so the ancestor's local id
/// falls out of the same subtraction.
fn supervisor_local_id(
    supervisor: &rhei_core::ast::Task,
    local_id: &str,
    task: &rhei_core::ast::Task,
) -> String {
    let qualified = task.id.to_string();
    let prefix = qualified.strip_suffix(local_id).unwrap_or("");
    supervisor.id.to_string().strip_prefix(prefix).unwrap_or(&supervisor.id.to_string()).to_string()
}

/// Fold this transition's supervision effects into the metadata about to be
/// written: the moving task's own phase, and the checkpoint its nearest
/// supervising ancestor is owed.
///
/// It runs on the shared path so `rhei run`'s auto-advance, `rhei transition`,
/// `rhei complete`, and a callback redirect maintain the barrier identically.
// §FS-rhei-supervision.2 §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.3
fn apply_supervision_transition(
    existing: Option<&Metadata>,
    move_: SupervisionTransition<'_>,
) -> Option<Metadata> {
    let from_supervises = supervise_kind_of(move_.machine, move_.from).is_some();
    let to_supervises = supervise_kind_of(move_.machine, move_.to).is_some();

    let mut updated: Option<Metadata> = None;

    if from_supervises && move_.from == move_.to {
        // §FS-rhei-supervision.3.1: the self-loop releases the subtree.
        updated =
            Some(record_supervision_release(updated.as_ref().or(existing), move_.metadata_key));
    } else if from_supervises {
        updated = clear_supervision_for_task(updated.as_ref().or(existing), move_.metadata_key);
    }
    if to_supervises && move_.from != move_.to {
        // §FS-rhei-supervision.3.1: entry holds, with no checkpoints yet.
        updated = Some(record_supervision_hold(
            updated.as_ref().or(existing),
            move_.metadata_key,
            None,
        ));
    }

    if let Some((supervisor, checkpoint)) = move_.checkpoint() {
        let supervisor_key = parse_task_id(&format!(
            "{}{}",
            move_.metadata_prefix,
            supervisor_local_id(supervisor, move_.local_id, move_.task)
        ));
        updated = Some(record_supervision_hold(
            updated.as_ref().or(existing),
            &supervisor_key,
            Some(&checkpoint),
        ));
    }

    updated
}

/// Whether this edge ends a supervisor's visit.
///
/// The release self-loop is the one non-terminal edge that drops a
/// `**Assignee:**`: the visit it was claimed for is over, and a claim that
/// outlived it would read as "the supervisor is working right now" — every
/// later descendant exit taken for the supervisor's own doing (§2.1), and the
/// supervisor itself never scheduled again.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.4
fn transition_ends_supervisor_visit(
    machine: &rhei_validator::StateMachine,
    from: &str,
    to: &str,
) -> bool {
    from == to && supervise_kind_of(machine, from).is_some()
}

/// Bind one applied transition on the shared path to the supervision rules.
///
/// The shared path knows the transitioning task, the files its rewrites land
/// in, and the move it is about to commit; everything supervision needs falls
/// out of those three. `move_` is `(local id, from, to)`, with `to` the
/// *effective* target after any callback redirect.
// §FS-rhei-supervision.2 §FS-rhei-supervision.3.3
fn supervision_after_transition(
    existing: Option<&Metadata>,
    machine: &rhei_validator::StateMachine,
    task_info: &TransitionTaskInfo,
    files: TransitionFiles<'_>,
    metadata_key: &TaskId,
    move_: (&str, &str, &str),
    to_visit: u64,
) -> Option<Metadata> {
    let (local_id, from, to) = move_;
    apply_supervision_transition(
        existing,
        SupervisionTransition {
            machine,
            task: &task_info.task,
            ancestors: &task_info.ancestors,
            metadata_key,
            metadata_prefix: files.metadata_id.strip_suffix(local_id).unwrap_or(""),
            local_id,
            from,
            to,
            to_visit,
        },
    )
}

// ---------------------------------------------------------------------------
// Readiness
// ---------------------------------------------------------------------------

/// What supervision says about scheduling one task. §FS-rhei-supervision.3.2
#[derive(Debug, Clone, PartialEq, Eq)]
enum SupervisionVerdict {
    /// Nothing on this task's path supervises: today's rules decide.
    Unsupervised,
    /// The task is in a supervising state and owed a visit, with nothing
    /// beneath it in flight. The "every descendant is terminal" rule does not
    /// apply to it.
    SupervisorReady,
    /// The task is in a supervising state but must not run: it has released
    /// its subtree, or something beneath it is still in flight.
    SupervisorWaiting,
    /// A supervising ancestor holds it.
    Held { supervisor: TaskId, state: String },
}

/// Index every task in the plan by id, for the ancestor walks readiness needs.
fn task_index<'a>(
    tasks: &[&'a rhei_core::ast::Task],
) -> std::collections::HashMap<TaskId, &'a rhei_core::ast::Task> {
    tasks.iter().map(|task| (task.id.clone(), *task)).collect()
}

/// Whether anything beneath `task` is being worked right now.
// §FS-rhei-supervision.3.2: in flight is a spawned-and-unexited run, or a claim.
fn any_descendant_in_flight(
    task: &rhei_core::ast::Task,
    in_flight: &dyn Fn(&rhei_core::ast::Task) -> bool,
) -> bool {
    task.children
        .iter()
        .any(|child| in_flight(child) || any_descendant_in_flight(child, in_flight))
}

/// Apply the hold/release rule to one task.
///
/// The ancestors are consulted first and all the way up: a supervisor higher in
/// the tree holds everything beneath it, nested supervisors included, so a
/// released inner supervisor cannot let its own children out from under a held
/// outer one.
// §FS-rhei-supervision.3.1 §FS-rhei-supervision.3.2
fn supervision_verdict(
    task: &rhei_core::ast::Task,
    index: &std::collections::HashMap<TaskId, &rhei_core::ast::Task>,
    machines: &rhei_validator::MachineSet,
    metadata: Option<&Metadata>,
    in_flight: &dyn Fn(&rhei_core::ast::Task) -> bool,
) -> SupervisionVerdict {
    let mut cursor = task.id.parent();
    while let Some(id) = cursor {
        let Some(ancestor) = index.get(&id) else { break };
        let machine = machines.for_task(&ancestor.id);
        if task_is_supervising(ancestor, machine)
            && (supervision_phase(metadata, &ancestor.id) == SupervisionPhase::Held
                || in_flight(ancestor))
        {
            return SupervisionVerdict::Held {
                supervisor: ancestor.id.clone(),
                state: normalized_state_name(ancestor.state.as_str(), machine),
            };
        }
        cursor = id.parent();
    }

    let machine = machines.for_task(&task.id);
    if !task_is_supervising(task, machine) {
        return SupervisionVerdict::Unsupervised;
    }
    // §FS-rhei-supervision.3.1: the drain — siblings already running finish
    // before the supervisor sees the checkpoints they produced.
    if any_descendant_in_flight(task, in_flight) {
        return SupervisionVerdict::SupervisorWaiting;
    }
    match supervision_phase(metadata, &task.id) {
        SupervisionPhase::Held => SupervisionVerdict::SupervisorReady,
        SupervisionPhase::Released => SupervisionVerdict::SupervisorWaiting,
    }
}

/// The verdict for a task whose in-flight set is whatever the plan shows: a
/// `**Assignee:**` claim, plus the ids a live run says it has spawned.
// §FS-rhei-supervision.3.2
fn supervision_verdict_for(
    task: &rhei_core::ast::Task,
    index: &std::collections::HashMap<TaskId, &rhei_core::ast::Task>,
    machines: &rhei_validator::MachineSet,
    metadata: Option<&Metadata>,
    spawned: &HashSet<String>,
) -> SupervisionVerdict {
    let in_flight = |candidate: &rhei_core::ast::Task| {
        candidate.assignee.is_some() || spawned.contains(&candidate.id.to_string())
    };
    supervision_verdict(task, index, machines, metadata, &in_flight)
}

/// Whether this task is work anyone can be handed right now, as far as the
/// subtree beneath it is concerned.
///
/// One answer for the ready set, `rhei next`, and `rhei list --ready`: a
/// supervisor owed a visit is work while its subtree is open, a released or
/// draining one is not, a held descendant is not, and anything unsupervised
/// keeps the non-leaf rule it always had.
// §FS-rhei-supervision.3.2
fn subtree_admits_to_ready_set(
    task: &rhei_core::ast::Task,
    index: &std::collections::HashMap<TaskId, &rhei_core::ast::Task>,
    machines: &rhei_validator::MachineSet,
    metadata: Option<&Metadata>,
    spawned: &HashSet<String>,
) -> bool {
    match supervision_verdict_for(task, index, machines, metadata, spawned) {
        SupervisionVerdict::SupervisorReady => true,
        SupervisionVerdict::SupervisorWaiting | SupervisionVerdict::Held { .. } => false,
        SupervisionVerdict::Unsupervised => descendants_are_terminal(task, machines),
    }
}

/// The supervisor holding `task`, when one does — for the surfaces that explain
/// why a ticket is not moving. §FS-rhei-supervision.3.4
fn held_by_supervisor(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> Option<(TaskId, String)> {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let index = task_index(&all);
    match supervision_verdict_for(
        task,
        &index,
        machines,
        rhei.metadata.as_ref(),
        &HashSet::new(),
    ) {
        SupervisionVerdict::Held { supervisor, state } => Some((supervisor, state)),
        _ => None,
    }
}
