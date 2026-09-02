// What a finished supervising visit released: the test the engine applies
// before it lets that visit's self-loop fire, and the warning it prints when the
// answer is "nothing".
//
// Its own part because it is the one supervision question that reads the whole
// subtree at once. The barrier next door decides what a single applied
// transition means; this decides whether a transition may be applied at all,
// and it is asked from both agent completion paths.

// §AR-source-file-size.3 §FS-rhei-supervision.3.6

/// A supervisor's subtree as one visit found it: every descendant's id with the
/// normalized state it was in, in preorder.
///
/// Captured at the spawn and compared after the exit, because "the visit moved
/// the subtree" is a statement about the span the subprocess ran for and about
/// nothing else. A checkpoint cannot answer it: a move the supervisor's own
/// worker makes is its own doing and delivers none (§FS-rhei-supervision.2.1).
// §FS-rhei-supervision.3.6
type SubtreeShape = Vec<(String, String)>;

/// The shape of `task`'s subtree right now. §FS-rhei-supervision.3.6
fn subtree_shape(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
) -> SubtreeShape {
    let mut all = Vec::new();
    collect_plan_tasks(&task.children, &mut all);
    all.iter()
        .map(|child| {
            (
                child.id.to_string(),
                normalized_state_name(child.state.as_str(), machines.for_task(&child.id)),
            )
        })
        .collect()
}

/// The subtree shape of the ticket a spawn is about to run, when that ticket is
/// a supervisor. `None` for every other ticket, so a non-supervising state
/// carries nothing and costs nothing.
// §FS-rhei-supervision.3.6
fn subtree_shape_before_visit(
    task: &rhei_core::ast::Task,
    machines: &rhei_validator::MachineSet,
    state_name: &str,
) -> Option<SubtreeShape> {
    execute_on_of(machines.for_task(&task.id), state_name)?;
    Some(subtree_shape(task, machines))
}

/// One supervisor and the subtree it holds, as the release test reads them.
///
/// Two callers ask the same question of it: the completion handlers, about the
/// visit that just ended, and the halt classifier, about a supervisor a *past*
/// release already stranded.
// §FS-rhei-supervision.3.6
struct SupervisedSubtree<'a> {
    /// The plan the question is asked of — for a finished visit, the plan as
    /// re-read after the subprocess exited (§FS-rhei-supervision.4.1).
    rhei: &'a rhei_core::ast::Rhei,
    machines: &'a rhei_validator::MachineSet,
    /// The roots a ready-set scan would resolve settings and artifacts against.
    roots: ReadySetRoots<'a>,
    supervisor: &'a rhei_core::ast::Task,
}

/// One finished supervising visit, as the release test reads it.
// §FS-rhei-supervision.3.6
struct FinishedVisit<'a> {
    /// The run's own root: where merged settings are read from, as the
    /// ready-set scan reads them.
    workspace_root: &'a Path,
    /// The plan as re-read after the subprocess exited (§FS-rhei-supervision.4.1).
    plan: &'a LoadedPlan,
    machines: &'a rhei_validator::MachineSet,
    /// The supervisor whose visit just ended, and the state it ran in.
    task_id: &'a TaskId,
    state: &'a str,
    /// Its subtree as the spawn found it. `None` leaves rule 2 unasked.
    before: Option<&'a SubtreeShape>,
}

/// The warning to print when this visit's self-loop must be withheld, or `None`
/// when the visit released something and the engine advances as it always has.
///
/// The three release rules of §FS-rhei-supervision.3.6, in order: there is no
/// subtree to release, the visit moved it, or it can still move without the
/// supervisor. Anything that is not a supervising state, not a self-loop, or
/// not answerable returns `None` — this rule only ever *withholds* an edge, so
/// every uncertainty resolves to today's behaviour.
// §FS-rhei-supervision.3.6
fn empty_supervising_visit(visit: FinishedVisit<'_>) -> Option<String> {
    let machine = visit.machines.for_task(visit.task_id);
    execute_on_of(machine, visit.state)?;
    let task = find_task_by_id(&visit.plan.rhei.tasks, visit.task_id)?;
    // Only the release edge is withheld. A conditioned exit — `visitCount >=
    // visits`, `openDescendants < 1` — is the machine's own decision.
    let selected = find_next_transition(task, &visit.plan.rhei, machine).ok().flatten()?;
    if normalized_state_name(&selected, machine) != visit.state {
        return None;
    }
    // Rule 1: nothing is being held, so nothing is released.
    let open = open_descendant_tasks(task, visit.machines);
    if open.is_empty() {
        return None;
    }
    // Rule 2: the visit steered the subtree; the engine does not ask how.
    if visit.before.is_some_and(|before| *before != subtree_shape(task, visit.machines)) {
        return None;
    }
    // Rule 3: something under the barrier can move once it lifts.
    let held = SupervisedSubtree {
        rhei: &visit.plan.rhei,
        machines: visit.machines,
        roots: ReadySetRoots {
            workspace_root: visit.workspace_root,
            task_roots: &visit.plan.task_roots,
        },
        supervisor: task,
    };
    if supervised_subtree_can_move(&held, &open) {
        return None;
    }
    Some(empty_visit_warning(visit.task_id, visit.state, &held, &open))
}

/// Whether any of `open` — the supervisor's non-terminal descendants — can move
/// once the barrier lifts. §FS-rhei-supervision.3.6 rule 3
fn supervised_subtree_can_move(
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> bool {
    let mut all = Vec::new();
    collect_plan_tasks(&held.rhei.tasks, &mut all);
    let states = plan_state_map(&all, held.machines);
    let subtree: HashSet<TaskId> = subtree_shape(held.supervisor, held.machines)
        .iter()
        .map(|(id, _)| parse_task_id(id))
        .collect();
    open.iter().any(|task| descendant_can_still_move(held, &states, &subtree, task))
}

/// Whether this non-terminal descendant can move once the barrier lifts —
/// either it is in the ready set with its supervisor read as released, or what
/// it waits on is not its supervisor's to give.
// §FS-rhei-supervision.3.6 rule 3
fn descendant_can_still_move(
    held: &SupervisedSubtree<'_>,
    states: &std::collections::HashMap<&TaskId, String>,
    subtree: &HashSet<TaskId>,
    task: &rhei_core::ast::Task,
) -> bool {
    let machine = held.machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    let Some(state_def) = machine.states.get(&state) else { return false };
    // A human owns the next move, and time owns a poll's: the run is waiting on
    // one of them, not stranded behind a supervisor that will not wake.
    if state_def.gating || state_def.poll.is_some() {
        return true;
    }
    let mut priors_satisfied = true;
    for prior in &task.prior {
        let Some(prior_state) = states.get(prior) else {
            priors_satisfied = false;
            continue;
        };
        if dependency_is_satisfied(prior_state, held.machines.for_task(prior)) {
            continue;
        }
        priors_satisfied = false;
        // Other work owns it: a prior outside this subtree, still open, is not
        // something the supervisor could have unblocked on this visit.
        if !subtree.contains(prior)
            && !is_terminal_state(prior_state, held.machines.for_task(prior))
        {
            return true;
        }
    }
    // A descendant held by a *nested* supervisor needs no special case: that
    // supervisor is itself a descendant, and it answers here for itself.
    priors_satisfied
        && state_inputs_exist_for_ready_set(
            held.roots.workspace_root,
            held.roots.artifact_root(&task.id.to_string()),
            held.rhei,
            machine,
            task,
            &state,
        )
}

/// The halt line an empty visit prints: what the visit did not do, what its
/// descendants are waiting for, and what the engine does about it.
///
/// The blocked descendants are named with the files they wait on, because that
/// is the sentence an operator can act on — the run's own diagnosis of the same
/// tickets says exactly this, and a warning that only said "released nothing"
/// would send them to the log the visit did not write.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
fn empty_visit_warning(
    task_id: &TaskId,
    state: &str,
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> String {
    format!(
        "  halting Task {task_id} in state '{state}': the visit released nothing — it moved no \
         descendant and left none able to move: {}. No transition fires and the visit is not \
         spent; the ticket stays in '{state}' and a later pass or a rerun visits it again.",
        format_blocked_descendants(held, open)
    )
}

/// The open descendants with what each is waiting for, capped at three with a
/// `(+N more)` tail — the shape every other ticket list in the run uses, plus
/// the files, because those are what an operator can act on.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
fn format_blocked_descendants(
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> String {
    let blocked: Vec<String> = open
        .iter()
        .take(3)
        .map(|task| {
            let machine = held.machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            let missing =
                missing_state_inputs_for_ready_set(&held.roots, held.rhei, machine, task, &state);
            match missing.is_empty() {
                true => format!("Task {} ({state})", task.id),
                false => format!("Task {} ({state}) waits on {}", task.id, missing.join(", ")),
            }
        })
        .collect();
    let more = open.len().saturating_sub(blocked.len());
    match more {
        0 => blocked.join("; "),
        more => format!("{} (+{more} more)", blocked.join("; ")),
    }
}

/// The reason a *released* supervisor is not moving, when nothing beneath it
/// can move either — a workspace stranded by an empty visit before
/// §FS-rhei-supervision.3.6 withheld that self-loop.
///
/// Nothing can wake it: a released supervisor is scheduled only by a descendant
/// checkpoint, and a subtree where nothing can move produces none. The run used
/// to read this as "not scheduled … rerun to pick it up", which is the one
/// remedy that provably does nothing.
// §FS-rhei-supervision.3.6 §FS-rhei-run-report.3.1
fn stranded_released_supervisor(
    held: &SupervisedSubtree<'_>,
    open: &[&rhei_core::ast::Task],
) -> Option<String> {
    if open.is_empty()
        || recorded_supervision_phase(held.rhei.metadata.as_ref(), &held.supervisor.id)
            != Some(SupervisionPhase::Released)
    {
        return None;
    }
    (!supervised_subtree_can_move(held, open)).then(|| format_blocked_descendants(held, open))
}
