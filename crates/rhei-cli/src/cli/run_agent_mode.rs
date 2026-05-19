
#[derive(Clone, Debug)]
struct SnapshotOverrideRunSelection {
    task_id: String,
    target_slug: String,
}

fn select_snapshot_override_run_invocation(
    machine: &rhei_validator::StateMachine,
    opts: &RunOptions,
    invocations: &[(String, String, String, ResolvedAgent)],
) -> MietteResult<Option<SnapshotOverrideRunSelection>> {
    if opts.snapshot_override_ref().is_none() {
        return Ok(None);
    }

    let mut candidates = Vec::new();
    for (task_id, _raw_state, current_state, resolved) in invocations {
        let declares_inherit = machine
            .states
            .get(current_state)
            .and_then(|state| state.snapshot.as_ref())
            .and_then(|snapshot| snapshot.inherit.as_ref())
            .is_some();
        if !declares_inherit {
            continue;
        }
        let target_slug = snapshot_target_slug_or_err(resolved)?;
        candidates.push(SnapshotOverrideRunSelection {
            task_id: task_id.clone(),
            target_slug,
        });
    }

    let mut selected = candidates.clone();
    if let Some(task_selector) = opts.snapshot_task_selector() {
        selected.retain(|candidate| candidate.task_id == task_selector);
    }
    if let Some(target_selector) = opts.snapshot_target_selector() {
        selected.retain(|candidate| candidate.target_slug == target_selector);
    }

    if selected.len() == 1 {
        return Ok(selected.pop());
    }

    let candidate_lines = format_snapshot_override_candidates(&candidates);
    if selected.is_empty() {
        return Err(miette!(
            "--from-snapshot did not match an active snapshot.inherit invocation; candidates:\n{}",
            candidate_lines
        ));
    }
    Err(miette!(
        "--from-snapshot is ambiguous; matched {} active snapshot.inherit invocations:\n{}\nretry with --task <id> and --target <slug>",
        selected.len(),
        format_snapshot_override_candidates(&selected)
    ))
}

fn format_snapshot_override_candidates(candidates: &[SnapshotOverrideRunSelection]) -> String {
    if candidates.is_empty() {
        return "  <none>".to_string();
    }
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| a.task_id.cmp(&b.task_id).then_with(|| a.target_slug.cmp(&b.target_slug)));
    sorted
        .iter()
        .map(|candidate| {
            format!("  task={} target={}", candidate.task_id, candidate.target_slug)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Agent-driven execution mode: spawn coding agents for tasks.
fn run_agent_mode(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    settings: &RheiSettings,
    opts: &RunOptions,
    max_parallel: usize,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent, RunSummary, TaskOutcome};
    use std::time::{Instant as TuiInstant, SystemTime};

    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let runtime_dir = workspace_root.join("runtime");

    let initial_total_tasks = {
        let loaded = load_plan(input)?;
        loaded.rhei.tasks.len()
    };
    let frontend_parallel = max_parallel.max(1).min(u16::MAX as usize) as u16;
    let frontend =
        start_run_frontend(&workspace_root, input, opts, frontend_parallel, initial_total_tasks);
    let sink = frontend.sink.clone();
    sink.emit(RunEvent::RunStarted {
        workspace: workspace_root.clone(),
        parallel: frontend_parallel,
        total_tasks: initial_total_tasks,
    });
    frontend.announce_dashboard();

    macro_rules! run_message {
        ($level:expr, $($arg:tt)*) => {{
            sink.emit(RunEvent::Message {
                level: $level,
                text: format!($($arg)*),
            });
        }};
    }

    macro_rules! run_info {
        ($($arg:tt)*) => {
            run_message!(MessageLevel::Info, $($arg)*);
        };
    }

    macro_rules! run_warn {
        ($($arg:tt)*) => {
            run_message!(MessageLevel::Warn, $($arg)*);
        };
    }

    macro_rules! run_error {
        ($($arg:tt)*) => {
            run_message!(MessageLevel::Error, $($arg)*);
        };
    }

    let loaded = load_plan(input)?;
    let initial_terminal_count = loaded
        .rhei
        .tasks
        .iter()
        .filter(|task| is_terminal_state(task.state.as_str(), machine))
        .count();
    run_info!(
        "Running {} '{}' with {} task(s) ({} terminal at start).",
        if workspace::is_workspace(input) { "workspace" } else { "plan" },
        loaded.rhei.title,
        loaded.rhei.tasks.len(),
        initial_terminal_count
    );
    run_info!("Initial states: {}", format_state_counts(&loaded.rhei));

    let mut agents_spawned = 0u32;
    let mut programs_spawned = 0u32;
    let mut callback_transitions_made = 0u32;
    let mut blocked_by_missing_program_outputs = false;
    let mut pass = 0u32;

    loop {
        let loaded = load_plan(input)?;
        let ready = find_runnable_tasks(&loaded.rhei, machine, &workspace_root);
        if ready.is_empty() {
            if !opts.dry_run() {
                if let Some(deadline) = earliest_pending_poll_deadline(&loaded.rhei, machine) {
                    let sleep_secs = deadline.saturating_sub(current_unix_secs()).max(1);
                    run_info!(
                        "No ready tasks; sleeping {}s until the next poll attempt.",
                        sleep_secs
                    );
                    std::thread::sleep(Duration::from_secs(sleep_secs));
                    continue;
                }
            }
            break;
        }

        pass += 1;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|task| is_terminal_state(task.state.as_str(), machine))
            .count();
        sink.emit(RunEvent::PassStarted {
            pass,
            ready: ready.iter().map(|t| t.id.to_string()).collect(),
        });
        run_info!(
            "\nPass {}: {} ready, {} terminal, {} total.",
            pass,
            ready.len(),
            terminal_count,
            loaded.rhei.tasks.len()
        );
        run_info!("Ready: {}", format_ready_tasks(&ready));

        // Collect tasks that can be advanced autonomously.
        let plan_title = loaded.rhei.title.clone();
        let mut agent_tasks: Vec<(String, String, String, ResolvedAgent)> = Vec::new();
        let mut program_tasks: Vec<(String, String, String, ResolvedProgram)> = Vec::new();
        let mut callback_tasks: Vec<(String, String, String)> = Vec::new();

        for task in &ready {
            let task_id_str = task.id.to_string();
            let current_state_raw = task.state.as_str().to_string();
            let current_state = normalized_state_name(&current_state_raw, machine);

            // Check for gating state.
            if machine.states.get(&current_state).map(|d| d.gating).unwrap_or(false) {
                run_info!(
                    "Task {} is in gating state '{}'. Waiting for human action.",
                    task_id_str,
                    current_state
                );
                continue;
            }

            let state_def = machine
                .states
                .get(&current_state)
                .ok_or_else(|| miette!("state '{}' missing from loaded machine", current_state))?;

            if state_def.program.is_some() {
                if opts.no_program() {
                    callback_tasks.push((task_id_str, current_state_raw, current_state));
                    continue;
                }

                if let Some(resolved) = resolve_program(machine, &current_state, settings, opts)? {
                    program_tasks.push((task_id_str, current_state_raw, current_state, resolved));
                }
            } else {
                let invocations =
                    resolve_agent_invocations(machine, &current_state, settings, opts)?;
                if invocations.is_empty() {
                    if opts.no_agent() {
                        callback_tasks.push((task_id_str, current_state_raw, current_state));
                        continue;
                    }
                    // Surface every remediation slot from spec
                    // §Resolution Order: `defaults.agent`, the state's
                    // `agent`, `models.<id>.default_agent`, and `--agent`.
                    // Mention the resolved model id when one is set so
                    // operators can locate `models.<id>.default_agent`.
                    let resolved_model = state_def
                        .model
                        .clone()
                        .or_else(|| settings.defaults.model.clone())
                        .or_else(|| settings.model.clone());
                    let model_remediation = match &resolved_model {
                        Some(id) => format!(
                            "models.{id}.default_agent in {}/.rhei/settings.json",
                            workspace_root.display()
                        ),
                        None => "models.<id>.default_agent (in settings.json)".to_string(),
                    };
                    let header = match &resolved_model {
                        Some(id) => format!("no agent configured for model '{id}'."),
                        None => "no agent configured.".to_string(),
                    };
                    return Err(miette!(
                        "{header}\n\nSet one of:\n  \u{2022} defaults.agent in {}/.rhei/settings.json or ~/.config/rhei/settings.json\n  \u{2022} the state's `agent:` in states.yaml\n  \u{2022} {model_remediation}\n  \u{2022} --agent <AGENT> on the rhei run command line (e.g. rhei run {} --agent claude-code)\n\nBuilt-in agents: claude-code, codex, gemini, cursor, kilocode, pi",
                        workspace_root.display(),
                        input.display()
                    ));
                }

                let pending = if state_def.outputs.is_empty() {
                    invocations
                } else {
                    invocations
                        .into_iter()
                        .filter(|resolved| {
                            !state_outputs_exist_for_resolved_invocation(
                                &workspace_root,
                                task,
                                &current_state,
                                task.state.as_str(),
                                machine,
                                loaded.rhei.metadata.as_ref(),
                                state_def,
                                resolved,
                            )
                        })
                        .collect::<Vec<_>>()
                };

                if pending.is_empty() {
                    callback_tasks.push((task_id_str, current_state_raw, current_state));
                    continue;
                }

                // Orchestrator Completion Authority: every invocation that
                // `rhei run` will actually spawn must resolve to a finite
                // timeout so that a non-returning agent cannot block forever.
                // Invocations whose outputs already exist have been filtered
                // out above and do not need a timeout. See
                // docs/functional-spec/rhei-agents.spec.md §Completion Authority /
                // §Completion Condition.
                if !opts.dry_run() {
                    for resolved in &pending {
                        ensure_orchestrator_timeout(resolved, &current_state)?;
                    }
                }

                for resolved in pending {
                    agent_tasks.push((
                        task_id_str.clone(),
                        current_state_raw.clone(),
                        current_state.clone(),
                        resolved,
                    ));
                }
            }
        }

        let mut advanced_any = false;

        // Handle callback-only tasks first (fast, synchronous).
        for (task_id_str, current_state_raw, current_state) in &callback_tasks {
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let task = match loaded.rhei.tasks.iter().find(|t| t.id == target_id) {
                Some(t) => t,
                None => continue,
            };
            let next_to = find_next_transition(task, &loaded.rhei, machine)?;
            let Some(to_state) = next_to else { continue };

            if opts.dry_run() {
                run_info!(
                    "{}",
                    format_dry_run_transition(task_id_str, current_state_raw, &to_state)
                );
                continue;
            }

            if record_poll_self_loop_if_needed(
                input,
                loaded.rhei.metadata.as_ref(),
                machine,
                task,
                current_state,
                &to_state,
            )? {
                run_info!(
                    "Task {} poll self-loop scheduled next attempt from '{}'",
                    task_id_str,
                    current_state_raw
                );
                advanced_any = true;
                callback_transitions_made += 1;
                continue;
            }

            let task_ids_before: BTreeSet<String> =
                loaded.rhei.tasks.iter().map(|existing| existing.id.to_string()).collect();
            let task_file = loaded.task_file(task_id_str, input);
            let metadata_file = if workspace::is_workspace(input) {
                input.join("index.rhei.md")
            } else {
                task_file.clone()
            };
            match execute_transition(
                TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
                callback_paths,
                machine,
                task_id_str,
                current_state,
                &to_state,
                opts.no_callbacks(),
            ) {
                Ok(()) => {
                    run_info!(
                        "Task {} transitioned: '{}' \u{2192} '{}'",
                        task_id_str,
                        current_state_raw,
                        to_state
                    );
                    advanced_any = true;
                    callback_transitions_made += 1;
                    let reloaded = load_plan(input)?;
                    let discovered = newly_discovered_tasks(&task_ids_before, &reloaded.rhei.tasks);
                    if !discovered.is_empty() {
                        run_info!(
                            "  Workspace expanded: discovered {} new task(s): {}",
                            discovered.len(),
                            discovered.join(", ")
                        );
                    }
                }
                Err(err) => {
                    run_warn!("warning: failed to advance Task {}: {}", task_id_str, err);
                }
            }
        }

        let program_tasks = {
            let mut filtered: Vec<(String, String, String, ResolvedProgram)> = Vec::new();
            let mut state_claimant: HashMap<String, String> = HashMap::new();
            let mut deferred: BTreeSet<String> = BTreeSet::new();
            for entry in program_tasks {
                let is_concurrent =
                    machine.states.get(&entry.2).map(|d| d.concurrent).unwrap_or(false);
                if is_concurrent {
                    filtered.push(entry);
                    continue;
                }
                match state_claimant.get(&entry.2) {
                    Some(claimant) if claimant == &entry.0 => filtered.push(entry),
                    Some(_) => {
                        deferred.insert(entry.0);
                    }
                    None => {
                        state_claimant.insert(entry.2.clone(), entry.0.clone());
                        filtered.push(entry);
                    }
                }
            }
            if !deferred.is_empty() {
                let deferred_vec: Vec<String> = deferred.iter().cloned().collect();
                run_info!(
                    "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                    deferred_vec.len(),
                    deferred_vec.join(", ")
                );
                sink.emit(RunEvent::TasksDeferred { pass, tasks: deferred_vec });
            }
            filtered
        };

        if !program_tasks.is_empty() {
            if opts.dry_run() {
                for (task_id_str, current_state_raw, current_state, resolved) in &program_tasks {
                    let loaded = load_plan(input)?;
                    let target_id = parse_task_id(task_id_str);
                    if let Some(task) = loaded.rhei.tasks.iter().find(|t| t.id == target_id) {
                        if let Some(to_state) = find_program_exit_transition(
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            0,
                        )? {
                            run_info!(
                                "{}",
                                format_dry_run_transition(
                                    task_id_str,
                                    current_state_raw,
                                    &to_state
                                )
                            );
                        }
                    }
                    let _ = resolved;
                }
                sink.emit(RunEvent::PassEnded { pass, progressed: false });
                break;
            }

            for (task_id_str, _current_state_raw, current_state, resolved) in &program_tasks {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
                let Some(task) = task else { continue };
                let render_context = RuntimeTemplateContext {
                    workspace_root: &workspace_root,
                    plan_path: &callback_paths.plan_path,
                    state_machine_path: callback_paths.state_machine_path.as_deref(),
                    plan_title: &plan_title,
                    task,
                    state_name: current_state,
                    current_state_raw: task.state.as_str(),
                    machine,
                    metadata: loaded.rhei.metadata.as_ref(),
                    target: None,
                    model: None,
                    model_provider: None,
                    model_name: None,
                    agent: None,
                    agent_mode: None,
                    tooling: None,
                };
                let log = program_log_path(&runtime_dir, task_id_str, current_state);

                run_info!("\nSpawning program for Task {}: {}", task_id_str, task.title);
                run_info!("  Log: {}", log.display());

                let started_at = TuiInstant::now();
                let started_wall = SystemTime::now();
                sink.emit(RunEvent::SlotAssigned {
                    slot: 0,
                    task: task_id_str.clone(),
                    from: task.state.as_str().to_string(),
                    to: current_state.clone(),
                    agent: None,
                    log_path: log.clone(),
                    started_at,
                    wall_clock: started_wall,
                });

                let spawn_result = spawn_and_wait_program(resolved, &render_context, &log);
                let duration_ms = started_at.elapsed().as_millis() as u64;
                let finished_wall = SystemTime::now();
                let (outcome, exit_code) = match &spawn_result {
                    Ok(program_outcome) if program_outcome.status.success() => {
                        (TaskOutcome::Completed, program_outcome.status.code())
                    }
                    Ok(program_outcome) => {
                        let code = program_outcome.status.code().unwrap_or(-1);
                        (
                            if program_outcome.timed_out {
                                TaskOutcome::TimedOut
                            } else {
                                TaskOutcome::Failed(format!("exit {code}"))
                            },
                            program_outcome.status.code(),
                        )
                    }
                    Err(err) => (TaskOutcome::Failed(err.to_string()), None),
                };
                sink.emit(RunEvent::SlotReleased {
                    slot: 0,
                    task: task_id_str.clone(),
                    from: task.state.as_str().to_string(),
                    to: current_state.clone(),
                    log_path: log.clone(),
                    outcome,
                    finished_at: TuiInstant::now(),
                    wall_clock: finished_wall,
                    exit_code,
                    duration_ms,
                });

                match spawn_result {
                    Ok(program_outcome) => {
                        programs_spawned += 1;
                        let mut reloaded = load_plan(input)?;
                        let task_after = reloaded.rhei.tasks.iter().find(|t| t.id == target_id);
                        let mut state_after =
                            task_after.map(|t| t.state.as_str()).unwrap_or("unknown").to_string();

                        if state_after != *current_state {
                            run_info!(
                                "  Task {} advanced: '{}' -> '{}'",
                                task_id_str,
                                current_state,
                                state_after
                            );
                            advanced_any = true;
                            continue;
                        }

                        if program_outcome.timed_out {
                            match fire_timeout_transition(
                                input,
                                machine,
                                callback_paths,
                                task_id_str,
                                current_state,
                                program_outcome.timeout_secs,
                                opts.no_callbacks(),
                            ) {
                                TimeoutTransitionOutcome::Fired => {}
                                TimeoutTransitionOutcome::NoRule => {
                                    run_warn!(
                                        "  warning: program for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                                        task_id_str,
                                        current_state
                                    );
                                }
                                TimeoutTransitionOutcome::Failed => {}
                            }
                            reloaded = load_plan(input)?;
                            state_after = reloaded
                                .rhei
                                .tasks
                                .iter()
                                .find(|t| t.id == target_id)
                                .map(|t| t.state.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            if state_after != *current_state {
                                run_info!(
                                    "  Task {} advanced: '{}' -> '{}'",
                                    task_id_str,
                                    current_state,
                                    state_after
                                );
                                advanced_any = true;
                                continue;
                            }
                            continue;
                        }

                        let exit_code = program_outcome.status.code().unwrap_or(-1);
                        if let Some(to_state) = find_program_exit_transition(
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            exit_code,
                        )? {
                            if exit_code == 0 && to_state != *current_state {
                                let missing_required_outputs = collect_missing_required_outputs(
                                    &workspace_root,
                                    machine,
                                    reloaded.rhei.metadata.as_ref(),
                                    task_after.unwrap_or(task),
                                    current_state,
                                );
                                if !missing_required_outputs.is_empty() {
                                    run_warn!(
                                        "  warning: program exited 0 but required outputs are missing for task {} in state '{}': {}",
                                        task_id_str,
                                        current_state,
                                        missing_required_outputs.join(", ")
                                    );
                                    blocked_by_missing_program_outputs = true;
                                    continue;
                                }
                            }
                            if record_poll_self_loop_if_needed(
                                input,
                                loaded.rhei.metadata.as_ref(),
                                machine,
                                task,
                                current_state,
                                &to_state,
                            )? {
                                run_info!(
                                    "  Task {} poll self-loop scheduled next attempt from '{}'",
                                    task_id_str,
                                    current_state
                                );
                                advanced_any = true;
                                continue;
                            }
                            let task_file = loaded.task_file(task_id_str, input);
                            let metadata_file = if workspace::is_workspace(input) {
                                input.join("index.rhei.md")
                            } else {
                                task_file.clone()
                            };
                            execute_system_program_exit_transition(
                                TransitionFiles {
                                    task_file: &task_file,
                                    metadata_file: &metadata_file,
                                },
                                callback_paths,
                                machine,
                                task_id_str,
                                current_state,
                                &to_state,
                                exit_code,
                                opts.no_callbacks(),
                            )?;
                            run_info!(
                                "  Task {} advanced: '{}' -> '{}'",
                                task_id_str,
                                current_state,
                                to_state
                            );
                            advanced_any = true;
                        } else if program_outcome.status.success() {
                            run_warn!(
                                "  warning: program exited 0 but task {} did not advance from '{}'",
                                task_id_str,
                                current_state
                            );
                        } else {
                            run_error!(
                                "  error: program exited with code {} for task {}",
                                exit_code,
                                task_id_str
                            );
                            if !opts.continue_on_error() {
                                return Err(miette!(
                                    "program exited with code {} for Task {}. Use --continue-on-error to skip failures.",
                                    exit_code,
                                    task_id_str
                                ));
                            }
                        }
                    }
                    Err(err) => {
                        run_error!("  error: {}", err);
                        if !opts.continue_on_error() {
                            return Err(err);
                        }
                    }
                }
            }
        }

        if agent_tasks.is_empty() {
            if !advanced_any {
                if opts.dry_run() {
                    sink.emit(RunEvent::PassEnded { pass, progressed: false });
                    break;
                }
                run_info!("No program, agent, or callback-only tasks could advance.");
                sink.emit(RunEvent::PassEnded { pass, progressed: false });
                break;
            }
            sink.emit(RunEvent::PassEnded { pass, progressed: true });
            continue;
        }

        // Enforce concurrent-state scheduling: for states without
        // `concurrent: true`, at most one task may be active in that state
        // per pass. Fanout invocations from the same task (via `all_targets`
        // / `all_models`) are always kept together. Deferred tasks are
        // naturally re-considered on the next pass.
        let agent_tasks = {
            let mut filtered: Vec<(String, String, String, ResolvedAgent)> = Vec::new();
            let mut state_claimant: HashMap<String, String> = HashMap::new();
            let mut deferred: BTreeSet<String> = BTreeSet::new();
            for entry in agent_tasks {
                let is_concurrent =
                    machine.states.get(&entry.2).map(|d| d.concurrent).unwrap_or(false);
                if is_concurrent {
                    filtered.push(entry);
                    continue;
                }
                match state_claimant.get(&entry.2) {
                    Some(claimant) if claimant == &entry.0 => filtered.push(entry),
                    Some(_) => {
                        deferred.insert(entry.0);
                    }
                    None => {
                        state_claimant.insert(entry.2.clone(), entry.0.clone());
                        filtered.push(entry);
                    }
                }
            }
            if !deferred.is_empty() {
                let deferred_vec: Vec<String> = deferred.iter().cloned().collect();
                run_info!(
                    "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                    deferred_vec.len(),
                    deferred_vec.join(", ")
                );
                sink.emit(RunEvent::TasksDeferred { pass, tasks: deferred_vec });
            }
            filtered
        };

        // Determine which task ids to schedule this pass. `--parallel`
        // counts tasks; fanout invocations for a selected task stay together.
        let task_limit = if max_parallel == 0 { usize::MAX } else { max_parallel };
        let mut selected_task_ids = HashSet::new();
        let mut batch: Vec<(String, String, String, ResolvedAgent)> = Vec::new();
        for entry in &agent_tasks {
            if selected_task_ids.contains(&entry.0) {
                batch.push(entry.clone());
            } else if selected_task_ids.len() < task_limit {
                selected_task_ids.insert(entry.0.clone());
                batch.push(entry.clone());
            }
        }
        let batch_size = batch.len();
        let snapshot_override_selection =
            select_snapshot_override_run_invocation(machine, opts, &agent_tasks)?;

        if opts.dry_run() {
            for (task_id_str, current_state_raw, current_state, resolved) in &batch {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                if let Some(task) = loaded.rhei.tasks.iter().find(|t| t.id == target_id) {
                    if let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? {
                        run_info!(
                            "{}",
                            format_dry_run_transition(task_id_str, current_state_raw, &to_state)
                        );
                    }
                }
                let _ = (current_state, resolved);
            }
            sink.emit(RunEvent::PassEnded { pass, progressed: false });
            break;
        }

        // Spawn agents (sequential or parallel).
        if batch_size == 1 {
            // Sequential: spawn one agent at a time.
            let (task_id_str, _current_state_raw, current_state, resolved) = &batch[0];
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
            let Some(task) = task else { continue };

            let tooling = resolve_tooling(machine, current_state, settings);
            let gate = gate_tooling_for_agent(resolved, &tooling);
            for warning in &gate.warnings {
                run_warn!("{warning}");
            }
            if !gate.required.is_empty() {
                let mcp_unavailable = unavailable_ids(&gate.required, ToolingKind::Mcp);
                let skill_unavailable = unavailable_ids(&gate.required, ToolingKind::Skill);
                let mut fired = false;
                if !mcp_unavailable.is_empty() {
                    match fire_tooling_unavailable_transition(
                        input,
                        machine,
                        callback_paths,
                        task_id_str,
                        current_state,
                        ToolingKind::Mcp,
                        &mcp_unavailable,
                        opts.no_callbacks(),
                    ) {
                        TimeoutTransitionOutcome::Fired => {
                            advanced_any = true;
                            fired = true;
                        }
                        TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
                    }
                }
                if !fired && !skill_unavailable.is_empty() {
                    match fire_tooling_unavailable_transition(
                        input,
                        machine,
                        callback_paths,
                        task_id_str,
                        current_state,
                        ToolingKind::Skill,
                        &skill_unavailable,
                        opts.no_callbacks(),
                    ) {
                        TimeoutTransitionOutcome::Fired => {
                            advanced_any = true;
                            fired = true;
                        }
                        TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
                    }
                }
                if !fired {
                    let message =
                        format_required_tooling_error(task_id_str, current_state, &gate.required);
                    run_error!("  error: {message}");
                    if !opts.continue_on_error() {
                        return Err(miette!("{message}"));
                    }
                }
                sink.emit(RunEvent::PassEnded { pass, progressed: advanced_any });
                if !advanced_any {
                    break;
                }
                continue;
            }
            let tooling = gate.tooling;
            let render_context = RuntimeTemplateContext {
                workspace_root: &workspace_root,
                plan_path: &callback_paths.plan_path,
                state_machine_path: callback_paths.state_machine_path.as_deref(),
                plan_title: &loaded.rhei.title,
                task,
                state_name: current_state,
                current_state_raw: task.state.as_str(),
                machine,
                metadata: loaded.rhei.metadata.as_ref(),
                target: resolved.target.as_ref(),
                model: resolved.model.as_deref(),
                model_provider: resolved.model_provider.as_deref(),
                model_name: resolved.model_name.as_deref(),
                agent: Some(resolved.agent.id()),
                agent_mode: resolved.mode.as_deref(),
                tooling: Some(&tooling),
            };
            let prompt = compose_agent_prompt(&render_context);
            let visit_count = render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                current_state,
                task.state.as_str(),
                machine,
            );
            let log = agent_log_path(
                &runtime_dir,
                task_id_str,
                current_state,
                resolved_agent_log_suffix(resolved, Some(visit_count)).as_deref(),
            );

            run_info!(
                "\nSpawning agent '{}' for Task {}: {}",
                resolved.agent.id(),
                task_id_str,
                task.title
            );
            if let Some(m) = &resolved.model {
                run_info!("  Model: {m}");
            }
            run_info!("  Log: {}", log.display());

            // Spec § Execution Loop step 3: if the state declares
            // `snapshot.inherit:`, resolve and preload the source snapshot
            // before spawning the agent. The actual preload is owned by
            // impl-rhei-snapshots; this hook pins the call site so the
            // orchestration ordering is encoded in code.
            let snapshot_preload = preload_snapshot_inherit_before_spawn(
                input,
                &workspace_root,
                machine,
                task,
                current_state,
                resolved,
                settings,
                visit_count,
                snapshot_override_selection.as_ref(),
                opts,
            )?;

            let started_at = TuiInstant::now();
            let started_wall = SystemTime::now();
            sink.emit(RunEvent::SlotAssigned {
                slot: 0,
                task: task_id_str.clone(),
                from: task.state.as_str().to_string(),
                to: current_state.clone(),
                agent: Some(resolved.agent.id().to_string()),
                log_path: log.clone(),
                started_at,
                wall_clock: started_wall,
            });

            let spawn_result = spawn_and_wait_agent(
                resolved,
                &prompt,
                &execution_workspace_root(&callback_paths.plan_path),
                &callback_paths.plan_path,
                callback_paths.state_machine_path.as_deref(),
                task_id_str,
                current_state,
                &tooling,
                &log,
                &runtime_dir,
                Some(&snapshot_preload),
                0,
                sink.clone(),
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let finished_wall = SystemTime::now();
            let (outcome, exit_code) = match &spawn_result {
                Ok(outcome) if outcome.status.success() => {
                    (TaskOutcome::Completed, outcome.status.code())
                }
                Ok(outcome) => {
                    let code = outcome.status.code().unwrap_or(-1);
                    (
                        if outcome.timed_out {
                            TaskOutcome::TimedOut
                        } else {
                            TaskOutcome::Failed(format!("exit {code}"))
                        },
                        outcome.status.code(),
                    )
                }
                Err(err) => (TaskOutcome::Failed(err.to_string()), None),
            };
            sink.emit(RunEvent::SlotReleased {
                slot: 0,
                task: task_id_str.clone(),
                from: task.state.as_str().to_string(),
                to: current_state.clone(),
                log_path: log.clone(),
                outcome,
                finished_at: TuiInstant::now(),
                wall_clock: finished_wall,
                exit_code,
                duration_ms,
            });

            match spawn_result {
                Ok(AgentSpawnOutcome { status, timed_out, timeout_secs }) => {
                    agents_spawned += 1;
                    let state_def = machine.states.get(current_state).ok_or_else(|| {
                        miette!("state '{}' missing from loaded machine", current_state)
                    })?;
                    let outputs_ok = status.success()
                        && state_outputs_exist_for_resolved_invocation(
                            &workspace_root,
                            task,
                            current_state,
                            task.state.as_str(),
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            state_def,
                            resolved,
                        );
                    let missing_required_outputs = if status.success() && !outputs_ok {
                        collect_missing_required_outputs_for_resolved_invocation(
                            &workspace_root,
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            resolved,
                        )
                    } else {
                        Vec::new()
                    };
                    let snapshot_completion = if timed_out {
                        SnapshotCompletion::Timeout
                    } else if outputs_ok {
                        SnapshotCompletion::Success
                    } else {
                        SnapshotCompletion::Failure
                    };
                    let failure_selected_to_state = if timed_out {
                        find_timeout_transition(machine, current_state)
                    } else if !status.success() {
                        find_program_exit_transition(
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            status.code().unwrap_or(-1),
                        )?
                    } else {
                        None
                    };
                    if !status.success() {
                        if let Err(err) = emit_snapshots_after_agent_exit(
                            &workspace_root,
                            machine,
                            settings,
                            task,
                            current_state,
                            failure_selected_to_state.as_deref(),
                            resolved,
                            &log,
                            visit_count,
                            snapshot_completion,
                            &snapshot_preload,
                        ) {
                            run_error!("  error: {}", err);
                            if !opts.continue_on_error() {
                                return Err(err);
                            }
                        }
                    }
                    let reloaded = load_plan(input)?;
                    let task_after = reloaded.rhei.tasks.iter().find(|t| t.id == target_id);
                    let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
                    let state_before = current_state.as_str();

                    if state_after != state_before {
                        if status.success() {
                            if let Some(task_for_snapshot) = task_after {
                                if let Err(err) = emit_snapshots_after_agent_exit(
                                    &workspace_root,
                                    machine,
                                    settings,
                                    task_for_snapshot,
                                    state_before,
                                    Some(state_after),
                                    resolved,
                                    &log,
                                    visit_count,
                                    snapshot_completion,
                                    &snapshot_preload,
                                ) {
                                    run_error!("  error: {}", err);
                                    if !opts.continue_on_error() {
                                        return Err(err);
                                    }
                                }
                            }
                        }
                        run_info!(
                            "  Task {} advanced: '{}' -> '{}'",
                            task_id_str,
                            state_before,
                            state_after
                        );
                        advanced_any = true;
                    } else if status.success() {
                        if !missing_required_outputs.is_empty() {
                            if let Some(task_for_snapshot) = task_after {
                                if let Err(err) = emit_snapshots_after_agent_exit(
                                    &workspace_root,
                                    machine,
                                    settings,
                                    task_for_snapshot,
                                    state_before,
                                    None,
                                    resolved,
                                    &log,
                                    visit_count,
                                    snapshot_completion,
                                    &snapshot_preload,
                                ) {
                                    run_error!("  error: {}", err);
                                    if !opts.continue_on_error() {
                                        return Err(err);
                                    }
                                }
                            }
                            emit_exit_zero_missing_required_outputs_warning(
                                task_id_str,
                                state_before,
                                &missing_required_outputs,
                                &sink,
                            );
                            blocked_by_missing_program_outputs = true;
                            break;
                        }
                        let pending_more = machine
                            .states
                            .get(state_before)
                            .map(|state_def| {
                                task_has_pending_agent_invocations(
                                    &workspace_root,
                                    task,
                                    state_before,
                                    task.state.as_str(),
                                    machine,
                                    loaded.rhei.metadata.as_ref(),
                                    state_def,
                                    settings,
                                )
                            })
                            .transpose()?
                            .unwrap_or(false);
                        if pending_more {
                            if let Some(task_for_snapshot) = task_after {
                                if let Err(err) = emit_snapshots_after_agent_exit(
                                    &workspace_root,
                                    machine,
                                    settings,
                                    task_for_snapshot,
                                    state_before,
                                    None,
                                    resolved,
                                    &log,
                                    visit_count,
                                    snapshot_completion,
                                    &snapshot_preload,
                                ) {
                                    run_error!("  error: {}", err);
                                    if !opts.continue_on_error() {
                                        return Err(err);
                                    }
                                }
                            }
                            break;
                        }
                        let mut emit_before_transition =
                            |task_for_snapshot: &rhei_core::ast::Task,
                             to_state: &str|
                             -> MietteResult<()> {
                                emit_snapshots_after_agent_exit(
                                    &workspace_root,
                                    machine,
                                    settings,
                                    task_for_snapshot,
                                    state_before,
                                    Some(to_state),
                                    resolved,
                                    &log,
                                    visit_count,
                                    snapshot_completion,
                                    &snapshot_preload,
                                )
                            };
                        match try_auto_advance_task(
                            input,
                            machine,
                            callback_paths,
                            task_id_str,
                            state_before,
                            opts.no_callbacks(),
                            Some(&mut emit_before_transition),
                        ) {
                            Ok(Some(to_state)) => {
                                run_info!(
                                    "  Task {} auto-advanced: '{}' -> '{}'",
                                    task_id_str,
                                    state_before,
                                    to_state
                                );
                                advanced_any = true;
                            }
                            Ok(None) => {
                                if let Some(task_for_snapshot) = task_after {
                                    if let Err(err) = emit_snapshots_after_agent_exit(
                                        &workspace_root,
                                        machine,
                                        settings,
                                        task_for_snapshot,
                                        state_before,
                                        None,
                                        resolved,
                                        &log,
                                        visit_count,
                                        snapshot_completion,
                                        &snapshot_preload,
                                    ) {
                                        run_error!("  error: {}", err);
                                        if !opts.continue_on_error() {
                                            return Err(err);
                                        }
                                    }
                                }
                                emit_exit_zero_warnings(
                                    &workspace_root,
                                    machine,
                                    loaded.rhei.metadata.as_ref(),
                                    task,
                                    task_id_str,
                                    state_before,
                                    &sink,
                                );
                            }
                            Err(err) => {
                                run_warn!(
                                    "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                                    task_id_str, state_before, err
                                );
                            }
                        }
                    } else if timed_out {
                        let duration = timeout_secs.map(format_duration_human).unwrap_or_default();
                        run_warn!("  agent timed out after {} for task {}", duration, task_id_str);
                        if let Some(to_state) = failure_selected_to_state.as_deref() {
                            match fire_selected_timeout_transition(
                                input,
                                machine,
                                callback_paths,
                                task_id_str,
                                state_before,
                                to_state,
                                timeout_secs,
                                opts.no_callbacks(),
                            ) {
                                TimeoutTransitionOutcome::Fired => advanced_any = true,
                                TimeoutTransitionOutcome::NoRule => {}
                                TimeoutTransitionOutcome::Failed => {}
                            }
                        } else {
                            {
                                run_warn!(
                                    "  warning: agent for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                                    task_id_str, state_before
                                );
                            }
                        }
                    } else {
                        let code = status.code().unwrap_or(-1);
                        run_error!(
                            "  error: agent exited with code {} for task {}",
                            code,
                            task_id_str
                        );
                        if let Some(to_state) = failure_selected_to_state.as_deref() {
                            match fire_agent_exit_transition(
                                input,
                                machine,
                                callback_paths,
                                task_id_str,
                                state_before,
                                to_state,
                                code,
                                opts.no_callbacks(),
                            ) {
                                TimeoutTransitionOutcome::Fired => advanced_any = true,
                                TimeoutTransitionOutcome::NoRule => {}
                                TimeoutTransitionOutcome::Failed => {}
                            }
                        } else if !opts.continue_on_error() {
                            return Err(miette!(
                                "agent '{}' exited with code {} for Task {}. \
                                 Use --continue-on-error to skip failures.",
                                resolved.agent.id(),
                                code,
                                task_id_str
                            ));
                        }
                    }
                }
                Err(err) => {
                    run_error!("  error: {}", err);
                    if !opts.continue_on_error() {
                        return Err(err);
                    }
                }
            }
        } else {
            // Parallel: spawn multiple agents using threads.
            let mut handles = Vec::new();

            for (slot_idx, (task_id_str, _current_state_raw, current_state, resolved)) in
                batch.iter().enumerate()
            {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
                let Some(task) = task else { continue };

                let tooling = resolve_tooling(machine, current_state, settings);
                let gate = gate_tooling_for_agent(resolved, &tooling);
                for warning in &gate.warnings {
                    run_warn!("{warning}");
                }
                if !gate.required.is_empty() {
                    let mcp_unavailable = unavailable_ids(&gate.required, ToolingKind::Mcp);
                    let skill_unavailable = unavailable_ids(&gate.required, ToolingKind::Skill);
                    let mut fired = false;
                    if !mcp_unavailable.is_empty() {
                        match fire_tooling_unavailable_transition(
                            input,
                            machine,
                            callback_paths,
                            task_id_str,
                            current_state,
                            ToolingKind::Mcp,
                            &mcp_unavailable,
                            opts.no_callbacks(),
                        ) {
                            TimeoutTransitionOutcome::Fired => {
                                advanced_any = true;
                                fired = true;
                            }
                            TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {
                            }
                        }
                    }
                    if !fired && !skill_unavailable.is_empty() {
                        match fire_tooling_unavailable_transition(
                            input,
                            machine,
                            callback_paths,
                            task_id_str,
                            current_state,
                            ToolingKind::Skill,
                            &skill_unavailable,
                            opts.no_callbacks(),
                        ) {
                            TimeoutTransitionOutcome::Fired => {
                                advanced_any = true;
                                fired = true;
                            }
                            TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {
                            }
                        }
                    }
                    if !fired {
                        let message = format_required_tooling_error(
                            task_id_str,
                            current_state,
                            &gate.required,
                        );
                        run_error!("  error: {message}");
                        if !opts.continue_on_error() {
                            return Err(miette!("{message}"));
                        }
                    }
                    continue;
                }
                let tooling = gate.tooling;
                let render_context = RuntimeTemplateContext {
                    workspace_root: &workspace_root,
                    plan_path: &callback_paths.plan_path,
                    state_machine_path: callback_paths.state_machine_path.as_deref(),
                    plan_title: &loaded.rhei.title,
                    task,
                    state_name: current_state,
                    current_state_raw: task.state.as_str(),
                    machine,
                    metadata: loaded.rhei.metadata.as_ref(),
                    target: resolved.target.as_ref(),
                    model: resolved.model.as_deref(),
                    model_provider: resolved.model_provider.as_deref(),
                    model_name: resolved.model_name.as_deref(),
                    agent: Some(resolved.agent.id()),
                    agent_mode: resolved.mode.as_deref(),
                    tooling: Some(&tooling),
                };
                let prompt = compose_agent_prompt(&render_context);
                let visit_count = render_visit_count(
                    loaded.rhei.metadata.as_ref(),
                    &task.id,
                    current_state,
                    task.state.as_str(),
                    machine,
                );
                let log = agent_log_path(
                    &runtime_dir,
                    task_id_str,
                    current_state,
                    resolved_agent_log_suffix(resolved, Some(visit_count)).as_deref(),
                );
                let working_dir = execution_workspace_root(&callback_paths.plan_path);
                let plan_path = callback_paths.plan_path.clone();
                let state_machine_path = callback_paths.state_machine_path.clone();
                let tid = task_id_str.clone();
                let sname = current_state.clone();

                run_info!(
                    "\nSpawning agent '{}' for Task {}: {} (parallel)",
                    resolved.agent.id(),
                    task_id_str,
                    task.title
                );
                run_info!("  Log: {}", log.display());

                // Spec § Execution Loop step 3: snapshot inherit preload runs
                // before the agent subprocess is spawned. See
                // `preload_snapshot_inherit_before_spawn` for the contract.
                let snapshot_preload = preload_snapshot_inherit_before_spawn(
                    input,
                    &workspace_root,
                    machine,
                    task,
                    current_state,
                    resolved,
                    settings,
                    visit_count,
                    snapshot_override_selection.as_ref(),
                    opts,
                )?;

                let slot = slot_idx.min(u16::MAX as usize) as u16;
                let from_state = task.state.as_str().to_string();
                let started_at = TuiInstant::now();
                let started_wall = SystemTime::now();
                sink.emit(RunEvent::SlotAssigned {
                    slot,
                    task: task_id_str.clone(),
                    from: from_state.clone(),
                    to: current_state.clone(),
                    agent: Some(resolved.agent.id().to_string()),
                    log_path: log.clone(),
                    started_at,
                    wall_clock: started_wall,
                });

                // Clone what we need for the thread.
                let resolved_for_thread = resolved.clone();
                let tooling_for_thread = tooling.clone();
                let sink_for_thread = sink.clone();
                let log_for_thread = log.clone();
                let log_for_result = log.clone();
                let from_for_thread = from_state;
                let to_for_thread = current_state.clone();
                let tid_for_event = task_id_str.clone();
                let runtime_dir_for_thread = runtime_dir.clone();
                let snapshot_preload_for_thread = snapshot_preload.clone();
                let snapshot_preload_for_result = snapshot_preload.clone();
                let visit_for_result = visit_count;
                let resolved_for_result = resolved.clone();

                let handle = std::thread::spawn(move || {
                    let resolved = resolved_for_thread;
                    let result = spawn_and_wait_agent(
                        &resolved,
                        &prompt,
                        &working_dir,
                        &plan_path,
                        state_machine_path.as_deref(),
                        &tid,
                        &sname,
                        &tooling_for_thread,
                        &log,
                        &runtime_dir_for_thread,
                        Some(&snapshot_preload_for_thread),
                        slot,
                        sink_for_thread.clone(),
                    );
                    let duration_ms = started_at.elapsed().as_millis() as u64;
                    let (outcome, exit_code) = match &result {
                        Ok(outcome) if outcome.status.success() => {
                            (TaskOutcome::Completed, outcome.status.code())
                        }
                        Ok(outcome) => {
                            let code = outcome.status.code().unwrap_or(-1);
                            (
                                if outcome.timed_out {
                                    TaskOutcome::TimedOut
                                } else {
                                    TaskOutcome::Failed(format!("exit {code}"))
                                },
                                outcome.status.code(),
                            )
                        }
                        Err(err) => (TaskOutcome::Failed(err.to_string()), None),
                    };
                    sink_for_thread.emit(RunEvent::SlotReleased {
                        slot,
                        task: tid_for_event,
                        from: from_for_thread,
                        to: to_for_thread,
                        log_path: log_for_thread,
                        outcome,
                        finished_at: TuiInstant::now(),
                        wall_clock: SystemTime::now(),
                        exit_code,
                        duration_ms,
                    });
                    (
                        tid,
                        sname,
                        resolved_for_result,
                        log_for_result,
                        snapshot_preload_for_result,
                        visit_for_result,
                        result,
                    )
                });
                handles.push(handle);
            }

            // Collect results.
            for handle in handles {
                let (task_id_str, state_name, resolved, log, snapshot_preload, visit_count, result) =
                    match handle.join() {
                        Ok(value) => value,
                        Err(_) => {
                            let err = miette!("agent thread panicked");
                            run_error!("  error: {}", err);
                            if !opts.continue_on_error() {
                                return Err(err);
                            }
                            continue;
                        }
                    };
                match result {
                    Ok(AgentSpawnOutcome { status, timed_out, timeout_secs }) => {
                        agents_spawned += 1;
                        let target_id = parse_task_id(&task_id_str);
                        let reloaded = load_plan(input)?;
                        let task_after = reloaded.rhei.tasks.iter().find(|t| t.id == target_id);
                        let mut missing_required_outputs = Vec::new();
                        let mut snapshot_completion_for_emit = None;
                        let mut failure_selected_to_state = None;
                        if let (Some(task_for_snapshot), Some(state_def)) =
                            (task_after, machine.states.get(state_name.as_str()))
                        {
                            let outputs_ok = status.success()
                                && state_outputs_exist_for_resolved_invocation(
                                    &workspace_root,
                                    task_for_snapshot,
                                    &state_name,
                                    &state_name,
                                    machine,
                                    reloaded.rhei.metadata.as_ref(),
                                    state_def,
                                    &resolved,
                                );
                            if status.success() && !outputs_ok {
                                missing_required_outputs =
                                    collect_missing_required_outputs_for_resolved_invocation(
                                        &workspace_root,
                                        machine,
                                        reloaded.rhei.metadata.as_ref(),
                                        task_for_snapshot,
                                        &state_name,
                                        &resolved,
                                    );
                            }
                            let snapshot_completion = if timed_out {
                                SnapshotCompletion::Timeout
                            } else if outputs_ok {
                                SnapshotCompletion::Success
                            } else {
                                SnapshotCompletion::Failure
                            };
                            failure_selected_to_state = if timed_out {
                                find_timeout_transition(machine, &state_name)
                            } else if !status.success() {
                                find_program_exit_transition(
                                    machine,
                                    reloaded.rhei.metadata.as_ref(),
                                    task_for_snapshot,
                                    &state_name,
                                    status.code().unwrap_or(-1),
                                )?
                            } else {
                                None
                            };
                            if !status.success() {
                                if let Err(err) = emit_snapshots_after_agent_exit(
                                    &workspace_root,
                                    machine,
                                    settings,
                                    task_for_snapshot,
                                    &state_name,
                                    failure_selected_to_state.as_deref(),
                                    &resolved,
                                    &log,
                                    visit_count,
                                    snapshot_completion,
                                    &snapshot_preload,
                                ) {
                                    run_error!("  error: {}", err);
                                    if !opts.continue_on_error() {
                                        return Err(err);
                                    }
                                }
                            }
                            snapshot_completion_for_emit = Some(snapshot_completion);
                        }
                        let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
                        if state_after != state_name {
                            if status.success() {
                                if let (Some(task_for_snapshot), Some(snapshot_completion)) =
                                    (task_after, snapshot_completion_for_emit)
                                {
                                    if let Err(err) = emit_snapshots_after_agent_exit(
                                        &workspace_root,
                                        machine,
                                        settings,
                                        task_for_snapshot,
                                        &state_name,
                                        Some(state_after),
                                        &resolved,
                                        &log,
                                        visit_count,
                                        snapshot_completion,
                                        &snapshot_preload,
                                    ) {
                                        run_error!("  error: {}", err);
                                        if !opts.continue_on_error() {
                                            return Err(err);
                                        }
                                    }
                                }
                            }
                            run_info!(
                                "  Task {} advanced: '{}' -> '{}'",
                                task_id_str,
                                state_name,
                                state_after
                            );
                            advanced_any = true;
                        } else if status.success() {
                            if !missing_required_outputs.is_empty() {
                                if let (
                                    Some(task_for_snapshot),
                                    Some(snapshot_completion),
                                ) = (task_after, snapshot_completion_for_emit)
                                {
                                    if let Err(err) = emit_snapshots_after_agent_exit(
                                        &workspace_root,
                                        machine,
                                        settings,
                                        task_for_snapshot,
                                        &state_name,
                                        None,
                                        &resolved,
                                        &log,
                                        visit_count,
                                        snapshot_completion,
                                        &snapshot_preload,
                                    ) {
                                        run_error!("  error: {}", err);
                                        if !opts.continue_on_error() {
                                            return Err(err);
                                        }
                                    }
                                }
                                emit_exit_zero_missing_required_outputs_warning(
                                    &task_id_str,
                                    &state_name,
                                    &missing_required_outputs,
                                    &sink,
                                );
                                blocked_by_missing_program_outputs = true;
                                continue;
                            }
                            let pending_more = reloaded
                                .rhei
                                .tasks
                                .iter()
                                .find(|t| t.id == target_id)
                                .and_then(|task| {
                                    machine.states.get(state_name.as_str()).map(|state_def| {
                                        task_has_pending_agent_invocations(
                                            &workspace_root,
                                            task,
                                            &state_name,
                                            task.state.as_str(),
                                            machine,
                                            reloaded.rhei.metadata.as_ref(),
                                            state_def,
                                            settings,
                                        )
                                    })
                                })
                                .transpose()?
                                .unwrap_or(false);
                            if pending_more {
                                if let (
                                    Some(task_for_snapshot),
                                    Some(snapshot_completion),
                                ) = (task_after, snapshot_completion_for_emit)
                                {
                                    if let Err(err) = emit_snapshots_after_agent_exit(
                                        &workspace_root,
                                        machine,
                                        settings,
                                        task_for_snapshot,
                                        &state_name,
                                        None,
                                        &resolved,
                                        &log,
                                        visit_count,
                                        snapshot_completion,
                                        &snapshot_preload,
                                    ) {
                                        run_error!("  error: {}", err);
                                        if !opts.continue_on_error() {
                                            return Err(err);
                                        }
                                    }
                                }
                                continue;
                            }
                            let auto_advance_result =
                                if let Some(snapshot_completion) = snapshot_completion_for_emit {
                                    let mut emit_before_transition =
                                        |task_for_snapshot: &rhei_core::ast::Task,
                                         to_state: &str|
                                         -> MietteResult<()> {
                                            emit_snapshots_after_agent_exit(
                                                &workspace_root,
                                                machine,
                                                settings,
                                                task_for_snapshot,
                                                &state_name,
                                                Some(to_state),
                                                &resolved,
                                                &log,
                                                visit_count,
                                                snapshot_completion,
                                                &snapshot_preload,
                                            )
                                        };
                                    try_auto_advance_task(
                                        input,
                                        machine,
                                        callback_paths,
                                        &task_id_str,
                                        &state_name,
                                        opts.no_callbacks(),
                                        Some(&mut emit_before_transition),
                                    )
                                } else {
                                    try_auto_advance_task(
                                        input,
                                        machine,
                                        callback_paths,
                                        &task_id_str,
                                        &state_name,
                                        opts.no_callbacks(),
                                        None,
                                    )
                                };
                            match auto_advance_result {
                                Ok(Some(to_state)) => {
                                    run_info!(
                                        "  Task {} auto-advanced: '{}' -> '{}'",
                                        task_id_str,
                                        state_name,
                                        to_state
                                    );
                                    advanced_any = true;
                                }
                                Ok(None) => {
                                    if let Some(task) =
                                        reloaded.rhei.tasks.iter().find(|t| t.id == target_id)
                                    {
                                        if let Some(snapshot_completion) =
                                            snapshot_completion_for_emit
                                        {
                                            if let Err(err) = emit_snapshots_after_agent_exit(
                                                &workspace_root,
                                                machine,
                                                settings,
                                                task,
                                                &state_name,
                                                None,
                                                &resolved,
                                                &log,
                                                visit_count,
                                                snapshot_completion,
                                                &snapshot_preload,
                                            ) {
                                                run_error!("  error: {}", err);
                                                if !opts.continue_on_error() {
                                                    return Err(err);
                                                }
                                            }
                                        }
                                        emit_exit_zero_warnings(
                                            &workspace_root,
                                            machine,
                                            reloaded.rhei.metadata.as_ref(),
                                            task,
                                            &task_id_str,
                                            &state_name,
                                            &sink,
                                        );
                                    } else {
                                        run_warn!(
                                            "  warning: agent exited 0 but task {} did not advance from '{}'",
                                            task_id_str, state_name
                                        );
                                    }
                                }
                                Err(err) => {
                                    run_warn!(
                                        "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                                        task_id_str, state_name, err
                                    );
                                }
                            }
                        } else if timed_out {
                            run_warn!(
                                "  agent timed out for task {} in '{}'",
                                task_id_str,
                                state_name
                            );
                            if let Some(to_state) = failure_selected_to_state.as_deref() {
                                match fire_selected_timeout_transition(
                                    input,
                                    machine,
                                    callback_paths,
                                    &task_id_str,
                                    &state_name,
                                    to_state,
                                    timeout_secs,
                                    opts.no_callbacks(),
                                ) {
                                    TimeoutTransitionOutcome::Fired => advanced_any = true,
                                    TimeoutTransitionOutcome::NoRule => {}
                                    TimeoutTransitionOutcome::Failed => {}
                                }
                            } else {
                                {
                                    run_warn!(
                                        "  warning: agent for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                                        task_id_str, state_name
                                    );
                                }
                            }
                        } else {
                            let code = status.code().unwrap_or(-1);
                            run_error!(
                                "  error: agent exited with code {} for task {}",
                                code,
                                task_id_str
                            );
                            if let Some(to_state) = failure_selected_to_state.as_deref() {
                                match fire_agent_exit_transition(
                                    input,
                                    machine,
                                    callback_paths,
                                    &task_id_str,
                                    &state_name,
                                    to_state,
                                    code,
                                    opts.no_callbacks(),
                                ) {
                                    TimeoutTransitionOutcome::Fired => advanced_any = true,
                                    TimeoutTransitionOutcome::NoRule => {}
                                    TimeoutTransitionOutcome::Failed => {}
                                }
                            } else if !opts.continue_on_error() {
                                return Err(miette!(
                                    "agent exited with code {code} for Task {task_id_str}. \
                                     Use --continue-on-error to skip failures."
                                ));
                            }
                        }
                    }
                    Err(err) => {
                        run_error!("  error for task {}: {}", task_id_str, err);
                        if !opts.continue_on_error() {
                            return Err(err);
                        }
                    }
                }
            }
        }

        sink.emit(RunEvent::PassEnded { pass, progressed: advanced_any });

        if !advanced_any {
            break;
        }
    }

    // Print summary.
    let (terminal_count, total_tasks) = if opts.dry_run() {
        // Spec §Dry-Run Output: final line reads "Dry run complete - no
        // agents were spawned." Programs are also skipped under --dry-run,
        // but the wording matches the agent-spec example so existing
        // tooling that greps for this exact phrase keeps working.
        run_info!("\nDry run complete - no agents were spawned.");
        (0usize, 0usize)
    } else if agents_spawned == 0 && programs_spawned == 0 {
        if callback_transitions_made == 0 {
            run_info!("No tasks could be advanced.");
            (0usize, 0usize)
        } else {
            let loaded = load_plan(input)?;
            let terminal_count = loaded
                .rhei
                .tasks
                .iter()
                .filter(|t| is_terminal_state(t.state.as_str(), machine))
                .count();
            run_info!(
                "\nRun complete: {} callback transition(s), {}/{} tasks in terminal state.",
                callback_transitions_made,
                terminal_count,
                loaded.rhei.tasks.len()
            );
            run_info!("Final states: {}", format_state_counts(&loaded.rhei));
            for task in &loaded.rhei.tasks {
                run_info!("  - {} [{}]", format_task_label(task), task.state);
            }
            (terminal_count, loaded.rhei.tasks.len())
        }
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|t| is_terminal_state(t.state.as_str(), machine))
            .count();
        run_info!(
            "\nRun complete: {} agent(s), {} program(s) spawned, {}/{} tasks in terminal state.",
            agents_spawned,
            programs_spawned,
            terminal_count,
            loaded.rhei.tasks.len()
        );
        run_info!("Final states: {}", format_state_counts(&loaded.rhei));
        for task in &loaded.rhei.tasks {
            run_info!("  - {} [{}]", format_task_label(task), task.state);
        }
        (terminal_count, loaded.rhei.tasks.len())
    };

    sink.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned,
            programs_spawned,
            terminal_tasks: terminal_count,
            total_tasks,
        },
    });
    drop(sink);
    drop(frontend);

    if !opts.dry_run() {
        let loaded = load_plan(input)?;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|task| is_terminal_state(task.state.as_str(), machine))
            .count();
        if terminal_count < loaded.rhei.tasks.len()
            && !remaining_work_is_only_gating_or_poll_blocked(&loaded.rhei, machine)
            && !blocked_by_missing_program_outputs
        {
            return Err(miette!(
                "rhei run halted with non-terminal tasks remaining and no further advancement possible"
            ));
        }
    }

    Ok(())
}
