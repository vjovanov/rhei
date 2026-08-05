
/// Execute the `complete` subcommand: transition a task to a terminal state,
/// write the central state ledger and result artifact, link it from the task
/// body, and remove the assignee.
///
/// The target terminal state is chosen automatically: the first non-cancelled
/// terminal state reachable from the task's current state via a declared
/// transition. If no such transition exists, the command fails.
fn complete_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    result_msg: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    // No `--rhei` on this command: the explicit ticket target is the scope.
    let task_id_str = &resolve_cli_task_id(&loaded, task_id_str, &None)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    // Find the task and its current state.
    let target_id = parse_task_id(task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
    let current_state_raw = task.state.as_str();
    let current_state = normalized_state_name(current_state_raw, &machine);

    // Reject tasks already in a terminal state.
    if is_terminal_state(current_state_raw, &machine) {
        return Err(miette!(
            "Task {} is already in terminal state '{}'",
            task_id_str,
            current_state_raw
        ));
    }
    if machine.states.get(&current_state).map(|def| def.gating).unwrap_or(false) {
        return Err(miette!(
            "Task {} cannot be completed from gating state '{}'; use an explicit human transition",
            task_id_str,
            current_state
        ));
    }

    let open_children = non_terminal_descendants(task, &machine);
    if !open_children.is_empty() {
        return Err(miette!(
            "Task {} cannot be completed while child tasks remain non-terminal.\nOffending children: {}",
            task_id_str,
            open_children.join(", ")
        ));
    }

    // Find the completion target: a non-cancelled terminal state reachable via
    // a single declared transition from the current state.
    let to_state = find_completion_state(&current_state, &machine).ok_or_else(|| {
        miette!(
            "no transition to a terminal state available from '{}' for Task {}",
            current_state_raw,
            task_id_str
        )
    })?;

    // Execute the state transition (compare-and-swap, callbacks, atomic write).
    let route = loaded.task_route(task_id_str, input);
    let effective_to = execute_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, artifact_root: &route.execution_root, artifact_id: task_id_str },
        &callback_paths,
        &machine,
        &route.local_id,
        &current_state,
        &to_state,
        no_callbacks,
    )?;
    if !is_successful_completion_state(&effective_to, &machine) {
        return Err(miette!(
            "Task {} was redirected to '{}', which is not a successful completion state; completion artifacts were not written",
            task_id_str,
            effective_to
        ));
    }

    // Append the completion entry to the result file in the owning rhei's
    // runtime, keyed by the project-qualified id. §AR-rhei-panta.2
    let root = &route.execution_root;
    let result_link = format!("runtime/results/{}.md", task_id_str);
    append_result_entry(root, task_id_str, current_state_raw, &effective_to, Some(result_msg))?;

    // Post-transition: remove assignee and link the result file (first time only).
    rewrite_task_completion(&route.task_file, &route.local_id, task_id_str, &result_link, true)?;

    println!(
        "Task {} completed: '{}' → '{}' ({})",
        task_id_str, current_state_raw, effective_to, result_link
    );

    Ok(())
}

/// Execute the `reset` subcommand: restore every task in the tree to the
/// state machine's initial state.
///
/// For directory workspaces, this also removes the generated `runtime/`
/// directory so logs and artifacts do not survive the reset.
fn reset_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    rhei_scope: &[String],
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    report_panta_scope_narrowed(&loaded, "reset", &scope);
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let reset_summary = reset_initial_summary(&loaded.rhei, &resolved.machine)?;

    fn count_nodes(task: &rhei_core::ast::Task) -> usize {
        1 + task.children.iter().map(count_nodes).sum::<usize>()
    }
    let in_scope: Vec<&rhei_core::ast::Task> = loaded
        .rhei
        .tasks
        .iter()
        .filter(|task| task_in_rhei_scope(&scope, &task.id.to_string()))
        .collect();
    let task_count = in_scope.len();
    let total_nodes: usize = in_scope.iter().map(|task| count_nodes(task)).sum();
    let descendant_count = total_nodes.saturating_sub(task_count);

    for file in reset_target_files(&loaded, input, &scope) {
        reset_plan_file_states(&file, &resolved.machine)?;
    }
    if workspace::is_workspace(input) {
        clear_runtime_metadata_in_file(&input.join("index.rhei.md"), true)?;
    }

    // §FS-rhei-panta.6.4: a narrowed reset removes per-ticket artifacts, never
    // whole `runtime/` trees — sibling rheis share one execution root.
    if scope.is_some() {
        let removed =
            remove_scoped_runtime_artifacts(&loaded, input, &scope, &resolved.machine)?;
        report_reset_summary(task_count, descendant_count, &reset_summary, removed);
        // A narrowed reset can only speak for ticket-owned artifacts; run-scoped
        // rollups belong to the run, not the ticket. Say so rather than leaving
        // the operator to discover the difference. §FS-rhei-panta.6.4
        println!(
            "Kept run-scoped output not owned by any ticket (run report, dashboard, \
             accounting rollups). Reset without `--rhei` to clear it."
        );
        return Ok(());
    }

    let mut runtime_dirs: Vec<PathBuf> = Vec::new();
    if loaded.is_panta_project() {
        let mut roots: BTreeSet<PathBuf> = loaded.task_roots.values().cloned().collect();
        roots.insert(input.to_path_buf());
        for root in roots {
            if workspace::is_workspace(&root) {
                clear_runtime_metadata_in_file(&root.join("index.rhei.md"), true)?;
            }
            runtime_dirs.push(root.join("runtime"));
        }
    } else if workspace::is_workspace(input) {
        runtime_dirs.push(input.join("runtime"));
    } else if let Some(parent) = input.parent() {
        runtime_dirs.push(parent.join("runtime"));
    }

    let mut removed_runtime = false;
    for runtime_dir in runtime_dirs {
        if runtime_dir.exists() {
            fs::remove_dir_all(&runtime_dir).map_err(|err| {
                file_io_report(&runtime_dir, "failed to remove runtime directory", err)
            })?;
            removed_runtime = true;
        }
    }

    report_reset_summary(task_count, descendant_count, &reset_summary, removed_runtime);
    Ok(())
}

fn report_reset_summary(
    task_count: usize,
    descendant_count: usize,
    reset_summary: &str,
    removed_runtime: bool,
) {
    if descendant_count == 0 {
        println!("Reset {} task(s) {}.", task_count, reset_summary);
    } else {
        println!(
            "Reset {} task(s) (and {} descendant task(s)) {}.",
            task_count, descendant_count, reset_summary
        );
    }
    if removed_runtime {
        println!("Removed runtime output.");
    } else {
        println!("No runtime output was present.");
    }
}

/// One runtime path a narrowed reset removes for a ticket: either a fully
/// resolved path, or a literal prefix within a directory when the artifact
/// template still carries run-time placeholders (`{state}`, `{visit_count}`,
/// `{model}`, …) that a reset cannot resolve.
enum ScopedTarget {
    Exact(PathBuf),
    Prefixed { dir: PathBuf, prefix: String },
}

/// Remove everything keyed by an in-scope ticket id — results, logs, declared
/// artifacts, snapshots, worktree refs, accounting, ledger lines — and nothing
/// else: sibling rheis share one execution root. §FS-rhei-reset.2.1
fn remove_scoped_runtime_artifacts(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<bool> {
    let mut removed = false;
    let mut task_ids: Vec<String> = Vec::new();
    fn collect(task: &rhei_core::ast::Task, out: &mut Vec<String>) {
        out.push(task.id.to_string());
        for child in &task.children {
            collect(child, out);
        }
    }
    for task in &loaded.rhei.tasks {
        collect(task, &mut task_ids);
    }

    // Ledger lines are pruned per execution root, once, after the per-ticket
    // sweep: sibling rheis share one `state-transitions.log`.
    let mut ledger_roots: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();

    for task_id in task_ids.iter().filter(|id| task_in_rhei_scope(scope, id)) {
        let root = loaded.task_root(task_id, input);
        let runtime = root.join("runtime");
        ledger_roots.entry(root).or_default().insert(task_id.clone());
        if !runtime.exists() {
            continue;
        }
        for target in scoped_runtime_targets(&runtime, task_id, machine) {
            removed |= remove_scoped_target(&target)?;
        }
    }

    for (root, ids) in ledger_roots {
        removed |= prune_transition_ledger(&root, &ids)?;
    }
    Ok(removed)
}

/// Every runtime path keyed by `task_id` under one execution root's `runtime/`.
fn scoped_runtime_targets(
    runtime: &Path,
    task_id: &str,
    machine: &rhei_validator::StateMachine,
) -> Vec<ScopedTarget> {
    let accounting_id = safe_accounting_file_segment(task_id);
    let mut targets = vec![
        // §FS-rhei-complete.4: the completion result file.
        ScopedTarget::Exact(runtime.join("results").join(format!("{task_id}.md"))),
        // §FS-rhei-agents.9 / §FS-rhei-programs.5: `task-<id>-<state>[-…].log`.
        ScopedTarget::Prefixed {
            dir: runtime.join("logs"),
            prefix: format!("task-{task_id}-"),
        },
        // §FS-rhei-snapshots.4: `<id>-<state>-<slug>-<nonce>/` session dirs.
        ScopedTarget::Prefixed {
            dir: runtime.join("snapshot-sessions"),
            prefix: format!("{task_id}-"),
        },
        ScopedTarget::Exact(runtime.join("worktree-refs").join(format!("{task_id}.yaml"))),
        // §FS-rhei-cost-accounting.2: per-ticket captures and task index.
        ScopedTarget::Prefixed {
            dir: runtime.join("accounting").join("captures"),
            prefix: format!("{accounting_id}-"),
        },
        ScopedTarget::Exact(
            runtime.join("accounting").join("tasks").join(format!("{accounting_id}.json")),
        ),
    ];

    // §FS-rhei-states.6: artifact contracts are the machine's own declaration
    // of what a ticket writes, so a reset that leaves them behind would let a
    // stale output satisfy a required input on the next run.
    let root = runtime.parent().unwrap_or(runtime);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for state in machine.states.values() {
        for artifact in state.inputs.iter().chain(state.outputs.iter()) {
            if !artifact.path.contains("{task_id}") || !seen.insert(artifact.path.clone()) {
                continue;
            }
            let resolved = artifact.path.replace("{task_id}", task_id);
            match resolved.split_once('{') {
                // A template with placeholders a reset cannot resolve becomes a
                // literal prefix; the text between `{task_id}` and the next
                // placeholder keeps `auth.1` from matching `auth.10`.
                Some((literal, _)) => {
                    let literal = root.join(literal);
                    let Some(dir) = literal.parent().map(Path::to_path_buf) else { continue };
                    let Some(prefix) =
                        literal.file_name().and_then(|name| name.to_str()).map(str::to_string)
                    else {
                        continue;
                    };
                    if !prefix.is_empty() {
                        targets.push(ScopedTarget::Prefixed { dir, prefix });
                    }
                }
                None => targets.push(ScopedTarget::Exact(root.join(resolved))),
            }
        }
    }
    targets
}

fn remove_scoped_target(target: &ScopedTarget) -> MietteResult<bool> {
    match target {
        ScopedTarget::Exact(path) => remove_runtime_path(path),
        ScopedTarget::Prefixed { dir, prefix } => {
            if !dir.is_dir() {
                return Ok(false);
            }
            let mut removed = false;
            for entry in fs::read_dir(dir)
                .map_err(|err| file_io_report(dir, "failed to read runtime directory", err))?
                .flatten()
            {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with(prefix.as_str()) {
                    removed |= remove_runtime_path(&entry.path())?;
                }
            }
            Ok(removed)
        }
    }
}

fn remove_runtime_path(path: &Path) -> MietteResult<bool> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|err| file_io_report(path, "failed to remove runtime directory", err))?;
        return Ok(true);
    }
    if path.exists() {
        fs::remove_file(path)
            .map_err(|err| file_io_report(path, "failed to remove runtime artifact", err))?;
        return Ok(true);
    }
    Ok(false)
}

/// Drop the in-scope tickets' lines from one execution root's transition
/// ledger, so a reset ticket's recorded history matches its plan state.
/// Lines read `<task-id> <from>@<to>`. §FS-rhei-panta.6.4
fn prune_transition_ledger(root: &Path, task_ids: &BTreeSet<String>) -> MietteResult<bool> {
    let ledger = root.join("runtime").join("state-transitions.log");
    if !ledger.is_file() {
        return Ok(false);
    }
    let raw = fs::read_to_string(&ledger)
        .map_err(|err| file_io_report(&ledger, "failed to read state transition log", err))?;
    let kept: Vec<&str> = raw
        .lines()
        .filter(|line| {
            let id = line.split_whitespace().next().unwrap_or_default();
            !task_ids.contains(id)
        })
        .collect();
    if kept.len() == raw.lines().count() {
        return Ok(false);
    }
    if kept.is_empty() {
        fs::remove_file(&ledger)
            .map_err(|err| file_io_report(&ledger, "failed to remove state transition log", err))?;
        return Ok(true);
    }
    let mut content = kept.join("\n");
    content.push('\n');
    write_file_atomic(&ledger, &content)?;
    Ok(true)
}

fn reset_initial_summary(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<String> {
    fn collect(
        task: &rhei_core::ast::Task,
        machine: &rhei_validator::StateMachine,
        states: &mut BTreeSet<String>,
    ) -> MietteResult<()> {
        states.insert(initial_state_for_node(machine, &task.kind, task.profile_level())?);
        for child in &task.children {
            collect(child, machine, states)?;
        }
        Ok(())
    }

    let mut states = BTreeSet::new();
    for task in &rhei.tasks {
        collect(task, machine, &mut states)?;
    }

    match states.len() {
        0 => Ok("to resolved initial states".to_string()),
        1 => Ok(format!("to initial state '{}'", states.iter().next().expect("one state"))),
        _ => Ok(format!(
            "to resolved profile initial states ({})",
            states.into_iter().collect::<Vec<_>>().join(", ")
        )),
    }
}

fn initial_state_for_node(
    machine: &rhei_validator::StateMachine,
    kind: &str,
    level: u8,
) -> MietteResult<String> {
    if let Some(profile) = machine.profile_for_node(kind, level) {
        return Ok(profile.initial.clone());
    }
    initial_state_name(machine)
}

fn initial_state_name(machine: &rhei_validator::StateMachine) -> MietteResult<String> {
    let initial_states = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    match initial_states.as_slice() {
        [] => Err(miette!("state machine '{}' does not declare an initial state", machine.name)),
        [initial] => Ok(initial.clone()),
        many => Err(miette!(
            "state machine '{}' declares multiple legacy initial states: {}",
            machine.name,
            many.join(", ")
        )),
    }
}
