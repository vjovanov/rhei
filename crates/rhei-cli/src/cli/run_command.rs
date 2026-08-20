/// Top-level tickets per owning plan file. Only top-level tickets are
/// independently schedulable — a subtask always executes inside its ticket's
/// slot — so descendants never add to a file's count. §FS-rhei-run.2.5
fn ticket_file_counts(loaded: &LoadedPlan) -> BTreeMap<&Path, usize> {
    let mut counts: BTreeMap<&Path, usize> = BTreeMap::new();
    for task in &loaded.rhei.tasks {
        if let Some(path) = loaded.task_sources.get(&task.id.to_string()) {
            *counts.entry(path.as_path()).or_default() += 1;
        }
    }
    counts
}

/// Every execution root a run locks: its own plus each rhei's, so project
/// and member-rhei runs contend on the same lock. Canonicalized and sorted —
/// one global acquisition order, no lock-order deadlock. §FS-rhei-run.2.6
fn run_lock_roots(loaded: &LoadedPlan, workspace_root: &Path) -> BTreeSet<PathBuf> {
    // Dedup is what keeps this set safe to lock: `flock` on a second
    // descriptor for an inode this process already holds blocks forever. A
    // ticket root can arrive as the empty path — `parent()` of a bare
    // relative filename — which canonicalizes to nothing and so slips past
    // dedup as a second name for the run's own directory.
    let canonical = |root: &Path| {
        let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
    };
    let mut roots = BTreeSet::new();
    roots.insert(canonical(workspace_root));
    for root in loaded.task_roots.values() {
        roots.insert(canonical(root));
    }
    roots
}

/// Execute the `run` subcommand: advance tasks through the state machine
/// in dependency order.
///
/// In agent mode (the default when an agent is configured), spawns coding
/// agents for each task. In callback-only mode (`--no-agent`), advances
/// tasks through transition callbacks only.
fn run_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    opts: RunOptions,
) -> MietteResult<()> {
    // Installed for every `run`, before anything can be spawned: from here on a
    // SIGINT/SIGTERM/SIGHUP interrupts the run instead of killing the
    // supervisor out from under its subprocesses. §FS-rhei-run.3.2
    install_interrupt_handlers();
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let rhei_scope = resolve_rhei_scope(&loaded, opts.rhei_scope())?;
    report_panta_scope_narrowed(&loaded, "run", &rhei_scope);
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input)?;
    let callback_paths = machines.default_callbacks.clone();
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;
    // §FS-rhei-run.2.6: one live run per rhei — lock every involved
    // execution root, not just the run's own.
    let _run_locks = if opts.dry_run() {
        Vec::new()
    } else {
        let mut locks = Vec::new();
        for root in run_lock_roots(&loaded, &workspace_root) {
            locks.push(acquire_run_lock(&root)?);
        }
        locks
    };
    // §FS-rhei-run.3.1: detect subprocess commits that leave run-owned state dirty.
    let git_consistency = RunGitConsistencyGuard::capture(&workspace_root, input, !opts.dry_run());

    // §FS-rhei-run.2.5: when every ticket lives in one plan file, parallelism
    // can only schedule same-file tickets against one checkout — fall back to
    // sequential, as for a bare single-file plan.
    let ticket_counts = ticket_file_counts(&loaded);
    let shared_file =
        ticket_counts.iter().find(|(_, count)| **count > 1).map(|(path, _)| (*path).to_path_buf());
    let single_shared_file = ticket_counts.len() == 1 && shared_file.is_some();
    let effective_parallel = if opts.parallel() > 1 && single_shared_file {
        eprintln!(
            "warning: --parallel > 1 is not supported when every ticket lives in one \
             plan file (risk of conflicting edits). Falling back to sequential execution."
        );
        1
    } else {
        // A project keeps parallelism, but two tickets of one rhei file are
        // still concurrent work against a single checkout — say so instead
        // of silently dropping the single-file warning. §FS-rhei-run.2.5
        if opts.parallel() > 1 {
            if let Some(shared) = &shared_file {
                eprintln!(
                    "warning: --parallel > 1 schedules tickets from the same rhei file \
                     concurrently ({}); plan-file writes serialize on the file lock, but \
                     agents may still collide in the shared checkout.",
                    shared.display()
                );
            }
        }
        opts.parallel()
    };

    // Initial validation pass.
    let mut report = rhei_validator::validate_with_machine_set(&loaded.rhei, &machines.set);
    for machine in machines.set.distinct() {
        report.errors.extend(validate_machine_settings_references(machine, &settings));
    }
    report
        .errors
        .extend(validate_task_execution_override_settings_references(&loaded.rhei, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved));
    if report.has_errors() {
        return Err(validation_report(input, resolved.default.path.as_deref(), &report.errors));
    }

    let use_standalone_mode =
        should_use_agent_mode(&loaded.rhei, &machines.set, &settings, &opts, &workspace_root)?;

    let result = if use_standalone_mode {
        run_agent_mode(input, &machines, &settings, &opts, effective_parallel)
    } else {
        run_callback_mode(input, &machines, &opts, effective_parallel)
    };
    result?;
    // An interrupted run made no claim of durable success, so the commit
    // postcondition has nothing to check; a run torn down by its own failure
    // still owes it. §FS-rhei-run.3.1 §FS-rhei-run.3.2
    if interrupted_by_signal() {
        return Ok(());
    }
    git_consistency.verify_after_success()
}

fn should_use_agent_mode(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
) -> MietteResult<bool> {
    if !opts.no_agent()
        && machines.distinct().iter().any(|machine| {
            machine
                .states
                .values()
                .any(|def| !def.terminal && !def.gating && state_declares_autonomous_agent_work(def))
        })
    {
        return Ok(true);
    }

    for task in narrow_to_rhei_scope(
        find_runnable_tasks(rhei, machines, workspace_root),
        &rhei_scope_set(opts.rhei_scope()),
    ) {
        let machine = machines.for_task(&task.id);
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let Some(def) = machine.states.get(&state_name) else {
            continue;
        };
        if def.terminal || def.gating {
            continue;
        }
        if def.program.is_some() && !opts.no_program() {
            return Ok(true);
        }
        if !opts.no_agent() {
            let invocations = resolve_agent_invocations_for_task(
                machine,
                &state_name,
                settings,
                opts,
                Some(task),
            )?;
            if !invocations.is_empty() || state_declares_autonomous_agent_work(def) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
