// Recovering the state a task was *authored* in, so `rhei reset` can put it
// back there instead of at the machine's `initial: true` state.
//
// Its own part because it is a read of runtime history, not a rewrite: the
// ledger it parses is deleted by the same command a moment later, so the read
// has to happen before the rewrite next door and cannot live inside it.

// §AR-source-file-size.3 §FS-rhei-reset.2.2

/// One task `rhei reset` moves back, as the preview and the summary print it.
/// §FS-rhei-reset.4
struct StateMove {
    /// Project-qualified, because that is the id the operator types.
    task_id: String,
    /// The state the plan holds now.
    from: String,
    /// The state the task was authored in.
    to: String,
}

/// Authored states ready for the rewrite: the file holding a task's heading,
/// then that task's *file-local* id — headings inside a rhei are rhei-local
/// even when the ledger keys are project-qualified. §AR-rhei-panta.3
type AuthoredByFile = BTreeMap<PathBuf, BTreeMap<String, String>>;

/// What reset learned from the ledgers before it deletes them.
struct AuthoredStates {
    by_file: AuthoredByFile,
    /// Every recovered move, sorted by task id.
    moves: Vec<StateMove>,
    /// Whether any in-scope execution root had a ledger at all. A plan that
    /// never ran and a plan whose `runtime/` was removed by hand are the same
    /// picture from here, and §FS-rhei-reset.2.2 leaves both alone.
    any_ledger: bool,
    /// With no ledger anywhere, the tasks sitting outside their profile's
    /// `initial` state — the ones an operator most likely expected to move.
    /// Empty whenever a ledger exists, because there absence of a line is a
    /// positive finding ("never moved") rather than missing information.
    stranded: Vec<(String, String)>,
}

/// Parse one execution root's ledger into `task-id → the first state it left`.
///
/// Lines are `<task-id> <from>@<to>` in the order the moves happened, so the
/// first `from` recorded for a task is the state that task started in. A
/// missing or unreadable ledger is not an error: it means no history, which
/// the caller answers by changing nothing.
// §FS-rhei-viz.4 §FS-rhei-reset.2.2
fn ledger_first_departures(root: &Path) -> BTreeMap<String, String> {
    let mut first: BTreeMap<String, String> = BTreeMap::new();
    let Ok(raw) = fs::read_to_string(root.join("runtime").join("state-transitions.log")) else {
        return first;
    };
    for line in raw.lines() {
        let mut fields = line.split_whitespace();
        let (Some(task_id), Some(movement)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Some((from, _to)) = movement.split_once('@') else {
            continue;
        };
        if from.is_empty() {
            continue;
        }
        // `or_insert`, never overwrite: later lines are later moves.
        first.entry(task_id.to_string()).or_insert_with(|| from.to_string());
    }
    first
}

/// Recover every in-scope task's authored state from the ledgers of the
/// execution roots that own them, and list the tasks that have moved away from
/// it. Reads only — the caller rewrites, then deletes the ledgers.
// §FS-rhei-reset.2.2
fn collect_authored_states(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
) -> AuthoredStates {
    let mut in_scope: Vec<String> = Vec::new();
    fn collect(task: &rhei_core::ast::Task, out: &mut Vec<String>) {
        out.push(task.id.to_string());
        for child in &task.children {
            collect(child, out);
        }
    }
    for task in &loaded.rhei.tasks {
        if task_in_rhei_scope(scope, &task.id.to_string()) {
            collect(task, &mut in_scope);
        }
    }

    // One read per execution root: sibling rheis share one ledger, and a task
    // graph can hold thousands of nodes.
    let mut ledgers: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();
    // A pre-qualification ledger keys by the rhei-local id, so falling back to
    // it is only unambiguous when exactly one in-scope task at that root wears
    // that local id. §FS-rhei-panta.6.4
    let mut local_id_owners: BTreeMap<(PathBuf, String), usize> = BTreeMap::new();

    let mut routes: Vec<(String, TaskRoute)> = Vec::with_capacity(in_scope.len());
    for task_id in &in_scope {
        let route = loaded.task_route(task_id, input);
        ledgers
            .entry(route.execution_root.clone())
            .or_insert_with(|| ledger_first_departures(&route.execution_root));
        *local_id_owners
            .entry((route.execution_root.clone(), route.local_id.clone()))
            .or_insert(0) += 1;
        routes.push((task_id.clone(), route));
    }
    let any_ledger = ledgers.values().any(|entries| !entries.is_empty());

    let current_states = current_states_by_id(&loaded.rhei, machines);
    let mut by_file: AuthoredByFile = BTreeMap::new();
    let mut moves: Vec<StateMove> = Vec::new();
    for (task_id, route) in routes {
        let ledger = ledgers.get(&route.execution_root).expect("ledger read above");
        let authored = ledger.get(&task_id).or_else(|| {
            let unambiguous = local_id_owners
                .get(&(route.execution_root.clone(), route.local_id.clone()))
                .is_some_and(|owners| *owners == 1);
            (route.local_id != task_id && unambiguous).then(|| ledger.get(&route.local_id))?
        });
        let Some(authored) = authored else {
            // No recorded history: the task never moved, so its `**State:**`
            // line already holds the authored state. §FS-rhei-reset.2.2
            continue;
        };
        by_file
            .entry(route.task_file.clone())
            .or_default()
            .insert(route.local_id.clone(), authored.clone());
        if let Some(current) = current_states.get(&task_id) {
            if current != authored {
                moves.push(StateMove {
                    task_id: task_id.clone(),
                    from: current.clone(),
                    to: authored.clone(),
                });
            }
        }
    }
    moves.sort_by(|a, b| a.task_id.cmp(&b.task_id));

    // Only meaningful with no ledger at all: that is the one case where
    // "no line" means "no information" instead of "never moved".
    let mut stranded: Vec<(String, String)> = Vec::new();
    if !any_ledger {
        for (task_id, current) in &current_states {
            let Some(task) = find_task(&loaded.rhei, task_id) else { continue };
            if !task_in_rhei_scope(scope, task_id) {
                continue;
            }
            let machine = machines.for_task_str(task_id);
            let initial = initial_state_for_node(machine, &task.kind, task.profile_level());
            if initial.is_ok_and(|initial| &initial != current) {
                stranded.push((task_id.clone(), current.clone()));
            }
        }
        stranded.sort();
    }

    AuthoredStates { by_file, moves, any_ledger, stranded }
}

/// Find one task anywhere in the merged graph by its qualified id.
fn find_task<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    task_id: &str,
) -> Option<&'a rhei_core::ast::Task> {
    fn walk<'a>(
        task: &'a rhei_core::ast::Task,
        task_id: &str,
    ) -> Option<&'a rhei_core::ast::Task> {
        if task.id.to_string() == task_id {
            return Some(task);
        }
        task.children.iter().find_map(|child| walk(child, task_id))
    }
    rhei.tasks.iter().find_map(|task| walk(task, task_id))
}

/// Every task's current state, normalized through its own rhei's machine so a
/// counted-visit suffix does not read as a different state than the bare name
/// the ledger records. §DA-per-rhei-state-machines
fn current_states_by_id(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> BTreeMap<String, String> {
    fn walk(
        task: &rhei_core::ast::Task,
        machine: &rhei_validator::StateMachine,
        out: &mut BTreeMap<String, String>,
    ) {
        out.insert(task.id.to_string(), normalized_state_name(task.state.as_str(), machine));
        for child in &task.children {
            walk(child, machine, out);
        }
    }
    let mut out = BTreeMap::new();
    for task in &rhei.tasks {
        walk(task, machines.for_task(&task.id), &mut out);
    }
    out
}

/// Print the moves a reset makes, under `verb` ("Would move" for the preview,
/// "Moved" for the summary). A count alone was true of the run that corrupted
/// a supervised chain and of the run that did nothing. §FS-rhei-reset.4
fn report_state_moves(authored: &AuthoredStates, verb: &str) {
    if authored.moves.is_empty() {
        // With no ledger anywhere, "no task had moved" would be a claim this
        // command cannot make: it would be reporting an absence of evidence as
        // evidence of absence. Say which one it is. §FS-rhei-reset.2.2
        if authored.any_ledger {
            println!("No task had moved from its authored state.");
        } else if authored.stranded.is_empty() {
            println!("No transition ledger, and every task is in its initial state.");
        } else {
            // Naming them is the whole recourse: with nothing recording where
            // these came from, only the operator knows. §FS-rhei-reset.2.2
            println!(
                "No transition ledger, so nothing records where these {} task(s) came \
                 from; they were left as authored:",
                authored.stranded.len()
            );
            for (task_id, state) in &authored.stranded {
                println!("  Task {task_id}: {state}");
            }
            println!(
                "Edit their **State:** lines directly if that is not where they should be."
            );
        }
        return;
    }
    println!("{} {} task(s) back:", verb, authored.moves.len());
    for mv in &authored.moves {
        println!("  Task {}: {} → {}", mv.task_id, mv.from, mv.to);
    }
}
