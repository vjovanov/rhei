/// Split the `<TICKET_OR_PLAN>` positional shared by every command that acts
/// on a single ticket between the two things it can name: a plan path or the
/// ticket id itself.
///
/// Returns the plan target (if one was named) and the ticket (if one was
/// resolved). A caller that requires a ticket reports its own absence, because
/// the sentence that helps depends on the command.
// §FS-rhei-usage.2: one ticket-argument shape across the command family.
fn split_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
) -> MietteResult<(Option<PathBuf>, Option<String>)> {
    // With --task present the positional is the plan path — legacy behavior.
    if let Some(task) = task {
        return Ok((input, Some(task)));
    }
    let Some(positional) = input else {
        return Ok((None, None));
    };
    // An existing path wins over id shape, so a plan named like an id never
    // silently selects a ticket. §FS-rhei-complete.2.1
    if positional.exists() {
        return Ok((Some(positional), None));
    }
    let raw = positional.to_string_lossy();
    if is_ticket_id_shaped(&raw) {
        return Ok((None, Some(raw.into_owned())));
    }
    Err(miette!(
help = io_error_help(&positional, std::io::ErrorKind::NotFound),
"plan '{}' does not exist", positional.display()))
}

/// Split `rhei complete`'s positional, where the ticket is mandatory.
// §FS-rhei-complete.2.1: `rhei complete auth.1 --result "…"` works as pasted.
fn split_complete_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
) -> MietteResult<(Option<PathBuf>, String)> {
    match split_ticket_target(input, task)? {
        (plan, Some(task)) => Ok((plan, task)),
        (Some(plan), None) => Err(miette!(
            help = ticket_id_required_help(),
            "'{}' is a plan path; name the ticket too: \
             `rhei complete <ticket-id> --result <message>` \
             (or `rhei complete {} --task <ticket-id> --result <message>`)",
            plan.display(),
            plan.display(),
        )),
        (None, None) => Err(miette!(
help = ticket_id_required_help(),

            "name the ticket to complete: `rhei complete <ticket-id> --result <message>` \
             (or `--task <ticket-id>`)"
        )),
    }
}

/// Split `rhei transition`'s positional, where the ticket is mandatory.
/// §FS-rhei-transition-cmd.1
fn split_transition_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
) -> MietteResult<(Option<PathBuf>, String)> {
    match split_ticket_target(input, task)? {
        (plan, Some(task)) => Ok((plan, task)),
        (Some(plan), None) => Err(miette!(
            help = ticket_id_required_help(),
            "'{}' is a plan path; name the ticket too: \
             `rhei transition <ticket-id> --from <state> --to <state>` \
             (or `rhei transition {} --task <ticket-id> --from <state> --to <state>`)",
            plan.display(),
            plan.display(),
        )),
        (None, None) => Err(miette!(
help = ticket_id_required_help(),

            "name the ticket to transition: \
             `rhei transition <ticket-id> --from <state> --to <state>` \
             (or `--task <ticket-id>`)"
        )),
    }
}

/// Whether `raw` has the shape of a ticket id (`3`, `auth.1`): dot-separated
/// segments that are numbers or names, no path separators, and not a markdown
/// file name. §FS-rhei-complete.2.1
fn is_ticket_id_shaped(raw: &str) -> bool {
    if raw.is_empty() || raw.contains(['/', '\\']) || raw.ends_with(".md") {
        return false;
    }
    raw.split('.').all(|segment| {
        let mut chars = segment.chars();
        match chars.next() {
            Some(c) if c.is_ascii_digit() => segment.chars().all(|c| c.is_ascii_digit()),
            Some(c) if c.is_ascii_alphabetic() => {
                chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            }
            _ => false,
        }
    })
}

/// Execute the `complete` subcommand: transition a task to a terminal state,
/// write the central state ledger and result artifact, link it from the task
/// body, and remove the assignee.
///
/// The target terminal state is chosen automatically: the first non-cancelled
/// terminal state reachable from the task's current state via a declared
/// transition. If no such transition exists, the command fails.
fn complete_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    result_msg: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    // No `--rhei` on this command: the explicit ticket target is the scope,
    // narrowed by the rhei the invocation was pointed at. §FS-rhei-panta.6
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let task_id_str = &resolve_cli_task_id(&loaded, task_id_str, &scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input)?;
    // One ticket is the whole scope: its machine and callback base govern.
    let machine = machines.for_task_str(task_id_str).clone();
    let callback_paths = machines.callbacks_for_str(task_id_str).clone();

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine_set(&loaded.rhei, &machines.set);
    if report.has_errors() {
        return Err(validation_report(input, resolved.default.path.as_deref(), &report.errors));
    }

    // Find the task and its current state.
    let target_id = parse_task_id(task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| miette!(
            help = task_id_help(),
            "task '{}' not found in the plan", task_id_str
        ))?;
    let current_state_raw = task.state.as_str();
    let current_state = normalized_state_name(current_state_raw, &machine);

    // Reject tasks already in a terminal state.
    if is_terminal_state(current_state_raw, &machine) {
        return Err(miette!(
            help = "nothing to do — the task is finished. Reopen it with: rhei reset <plan> <task>",
            "Task {} is already in terminal state '{}'",
            task_id_str,
            current_state_raw
        ));
    }
    if machine.states.get(&current_state).map(|def| def.gating).unwrap_or(false) {
        return Err(miette!(
            help = "a human gate is released explicitly: rhei transition <plan> <task> --to <state>",
            "Task {} cannot be completed from gating state '{}'; use an explicit human transition",
            task_id_str,
            current_state
        ));
    }

    // Descendants-first is not checked here: it is the shared transition path's
    // guard, so the rejection `rhei complete` produces is the one every other
    // verb produces. §FS-rhei-complete.4 §FS-rhei-transition-cmd.3.1

    // Completing ahead of a prerequisite makes the ticket terminal, which drops
    // it out of readiness and out of `rhei list --blocked` — the violation would
    // never surface again. §FS-rhei-complete.4
    let mut all_tasks = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut all_tasks);
    let state_map = plan_state_map(&all_tasks, &machines.set);
    let blocked_by = blocking_priors(task, &state_map, &machines.set);
    if !blocked_by.is_empty() {
        return Err(miette!(
help = "finish the blocking priors first, or move this ticket deliberately with: rhei transition <ticket-id> --from <state> --to <state>",

            "Task {} cannot be completed while its prerequisites are unsatisfied.\nBlocking priors: {}\n\
             Complete them first, or use `rhei transition` for a deliberate out-of-order move.",
            task_id_str,
            blocked_by.join(", ")
        ));
    }

    // Find the completion target: a non-cancelled terminal state reachable via
    // a single declared transition from the current state.
    let to_state = find_completion_state(&current_state, &machine).ok_or_else(|| {
        miette!(
            help = "the machine declares no terminal edge from that state. List the edges with: rhei states",
            "no transition to a terminal state available from '{}' for Task {}",
            current_state_raw,
            task_id_str
        )
    })?;

    // Execute the state transition (compare-and-swap, callbacks, atomic write).
    let route = loaded.task_route(task_id_str, input);
    let effective_to = execute_transition(
        TransitionFiles {
            task_file: &route.task_file,
            metadata_file: &route.metadata_file,
            metadata_id: &route.metadata_id,
            artifact_root: &route.execution_root,
            artifact_id: task_id_str,
        },
        &callback_paths,
        &machine,
        &route.local_id,
        &current_state,
        &to_state,
        no_callbacks,
    )?;
    if !is_successful_completion_state(&effective_to, &machine) {
        return Err(miette!(
            help = "inspect the machine and the task's state with: rhei states",
            "Task {} was redirected to '{}', which is not a successful completion state; completion artifacts were not written",
            task_id_str,
            effective_to
        ));
    }

    // Append the completion entry to the result file in the owning rhei's
    // runtime, keyed by the project-qualified id, then finalize the task.
    // §AR-rhei-panta.2
    record_transition_result(
        &route,
        &machine,
        task_id_str,
        current_state_raw,
        &effective_to,
        Some(result_msg),
    )?;

    let result_link = format!("runtime/results/{}.md", task_id_str);
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
    dry_run: bool,
    assume_yes: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    report_panta_scope_narrowed(&loaded, "reset", &scope);
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();
    let reset_summary = reset_initial_summary(&loaded.rhei, &machines, &scope)?;

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

    // Reset destroys result artifacts and ledgers that live under a `panta/`
    // directory `rhei init` gitignores by default — there is usually no VCS
    // copy to recover from. Show the damage before doing it.
    let runtime_targets = reset_runtime_preview(&loaded, input, &scope);
    // The preview precedes every destructive reset, not just the one that
    // stops to ask: printing it only on the interactive path left exactly the
    // unattended runs — scripts, CI, agents — silent. §FS-rhei-reset.1.2
    report_reset_preview(task_count, descendant_count, &reset_summary, &runtime_targets);
    if dry_run {
        println!("\nDry run — nothing was changed.");
        return Ok(());
    }
    if !assume_yes {
        // §FS-rhei-reset.1.2: with no terminal there is no one to answer, and
        // the destroyed material is typically gitignored with no VCS copy. Ask
        // for `-y` instead of assuming consent nobody gave.
        if !stdin_is_interactive() {
            return Err(miette!(
help = "re-run with -y to confirm, or --dry-run to preview what it would clear.",

                "`rhei reset` destroys runtime state and stdin is not a terminal, so it cannot \
                 ask for confirmation. Re-run with `-y` to confirm, or `--dry-run` to preview."
            ));
        }
        if !confirm("\nProceed?")? {
            println!("Cancelled — nothing was changed.");
            return Ok(());
        }
    }

    // Each plan file resets to the initial states of *its* rhei's machine.
    // §DA-per-rhei-state-machines
    for (file, sample_task_id) in reset_target_files(&loaded, input, &scope) {
        let rhei_id = sample_task_id.split('.').next().unwrap_or("");
        let machine =
            machines.per_rhei.get(rhei_id).unwrap_or(&machines.default);
        reset_plan_file_states(&file, machine)?;
    }
    if workspace::is_workspace(input) {
        clear_runtime_metadata_in_file(&input.join("index.rhei.md"), true)?;
    }

    // §FS-rhei-panta.6.4: a narrowed reset removes per-ticket artifacts, never
    // whole `runtime/` trees — sibling rheis share one execution root.
    if scope.is_some() {
        // §FS-rhei-panta.6.4: runtime ticket metadata (visit counts, poll
        // timers) in an in-scope workspace rhei's index is ticket-owned
        // state; leaving it would be a silent partial reset.
        let scoped_roots: BTreeSet<&PathBuf> = loaded
            .task_roots
            .iter()
            .filter(|(task_id, _)| task_in_rhei_scope(&scope, task_id))
            .map(|(_, root)| root)
            .collect();
        for root in scoped_roots {
            if workspace::is_workspace(root) && root.as_path() != input {
                clear_runtime_metadata_in_file(&root.join("index.rhei.md"), true)?;
            }
        }
        let removed = remove_scoped_runtime_artifacts(&loaded, input, &scope, &machines)?;
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

/// Runtime directories a full reset would delete, in report order. A narrowed
/// reset removes per-ticket artifacts rather than whole trees, so it lists
/// none and the preview says so in words. §FS-rhei-panta.6.4
fn reset_runtime_preview(loaded: &LoadedPlan, input: &Path, scope: &RheiScope) -> Vec<PathBuf> {
    if scope.is_some() {
        return Vec::new();
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if loaded.is_panta_project() {
        let mut roots: BTreeSet<PathBuf> = loaded.task_roots.values().cloned().collect();
        roots.insert(input.to_path_buf());
        dirs.extend(roots.into_iter().map(|root| root.join("runtime")));
    } else if workspace::is_workspace(input) {
        dirs.push(input.join("runtime"));
    } else if let Some(parent) = input.parent() {
        dirs.push(parent.join("runtime"));
    }
    dirs.retain(|dir| dir.exists());
    dirs
}

/// Describe what a reset is about to destroy.
fn report_reset_preview(
    task_count: usize,
    descendant_count: usize,
    reset_summary: &str,
    runtime_dirs: &[PathBuf],
) {
    if descendant_count == 0 {
        println!("Would reset {task_count} task(s) {reset_summary}.");
    } else {
        println!(
            "Would reset {task_count} task(s) and {descendant_count} subtask(s) {reset_summary}."
        );
    }
    if runtime_dirs.is_empty() {
        println!("Would remove per-ticket runtime artifacts (results, ledgers).");
    } else {
        println!("Would delete, with every result and ledger inside:");
        for dir in runtime_dirs {
            println!("  {}", dir.display());
        }
    }
}

/// True when there is a human on stdin to answer a prompt.
fn stdin_is_interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Ask a yes/no question, defaulting to no.
fn confirm(question: &str) -> MietteResult<bool> {
    use std::io::Write;
    print!("{question} [y/N] ");
    std::io::stdout().flush().map_err(|err| miette!(
help = internal_error_help(),
"failed to write prompt: {err}"))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|err| miette!(
help = "re-run with -y to confirm without a prompt.",
"failed to read confirmation: {err}"))?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
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
    machines: &rhei_validator::MachineSet,
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

    // Pre-qualification runtime records are keyed by the rhei-local id; a
    // local-id sweep at a root is only unambiguous when every rhei rooted
    // there is in scope — shared roots collide on local ids. §FS-rhei-panta.6.4
    let mut root_owners: BTreeMap<&PathBuf, BTreeSet<&str>> = BTreeMap::new();
    for (task_id, root) in &loaded.task_roots {
        let owner = task_id.split_once('.').map(|(head, _)| head).unwrap_or(task_id);
        root_owners.entry(root).or_default().insert(owner);
    }
    let legacy_sweep_ok = |root: &PathBuf| {
        root_owners
            .get(root)
            .is_some_and(|owners| owners.iter().all(|owner| task_in_rhei_scope(scope, owner)))
    };

    // Run-orchestrated logs and captures land under the project execution
    // root even for tickets whose own rhei root is a subdirectory, so a
    // narrowed reset must sweep both roots. §FS-rhei-reset.2.1
    let project_root = execution_workspace_root(input);
    for task_id in task_ids.iter().filter(|id| task_in_rhei_scope(scope, id)) {
        let root = loaded.task_root(task_id, input);
        let ledger_ids = ledger_roots.entry(root.clone()).or_default();
        ledger_ids.insert(task_id.clone());
        let local_id = rhei_local_id_str(task_id);
        if local_id != task_id && legacy_sweep_ok(&root) {
            ledger_ids.insert(local_id.to_string());
        }
        let mut base_roots = vec![root.clone()];
        if root != project_root {
            base_roots.push(project_root.clone());
        }
        for base in base_roots {
            let runtime = base.join("runtime");
            if !runtime.exists() {
                continue;
            }
            // Artifact-name patterns come from the owning ticket's machine.
            // §DA-per-rhei-state-machines
            let machine = machines.for_task_str(task_id);
            for target in scoped_runtime_targets(&runtime, task_id, machine) {
                removed |= remove_scoped_target(&target)?;
            }
            if local_id != task_id && legacy_sweep_ok(&base) {
                for target in scoped_runtime_targets(&runtime, local_id, machine) {
                    removed |= remove_scoped_target(&target)?;
                }
            }
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
        ScopedTarget::Prefixed { dir: runtime.join("logs"), prefix: format!("task-{task_id}-") },
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
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
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
    // Only the states this invocation will actually write: a narrowed reset
    // that summarized every machine's initial state named states no in-scope
    // ticket can reach. §FS-rhei-reset.2.1
    for task in &rhei.tasks {
        if !task_in_rhei_scope(scope, &task.id.to_string()) {
            continue;
        }
        collect(task, machines.for_task(&task.id), &mut states)?;
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
        [] => Err(miette!(
            help = state_machine_help(),
            "state machine '{}' does not declare an initial state", machine.name
        )),
        [initial] => Ok(initial.clone()),
        many => Err(miette!(
            help = state_machine_help(),
            "state machine '{}' declares multiple legacy initial states: {}",
            machine.name,
            many.join(", ")
        )),
    }
}
