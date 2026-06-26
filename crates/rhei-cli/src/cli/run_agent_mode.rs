
#[derive(Clone, Debug)]
struct SnapshotOverrideRunSelection {
    task_id: String,
    target_slug: String,
}

#[derive(Clone)]
struct AgentWorkItem {
    task_id_str: String,
    current_state_raw: String,
    current_state: String,
    resolved: ResolvedAgent,
}

#[derive(Clone)]
struct ProgramWorkItem {
    task_id_str: String,
    current_state: String,
    resolved: ResolvedProgram,
}

struct ParallelAgentCompletion {
    task_id_str: String,
    state_name: String,
    resolved: ResolvedAgent,
    log: PathBuf,
    snapshot_preload: SnapshotPreload,
    visit_count: u64,
    result: MietteResult<AgentSpawnOutcome>,
    accounting_recorded: bool,
    accounting_warning: Option<String>,
    slot: rhei_tui::Slot,
}

struct ParallelProgramCompletion {
    task_id_str: String,
    state_name: String,
    result: MietteResult<ProgramSpawnOutcome>,
    slot: rhei_tui::Slot,
}

enum ParallelAgentThreadMessage {
    Completed(ParallelAgentCompletion),
    ProgramCompleted(ParallelProgramCompletion),
    Panicked { task_id_str: String, state_name: String, slot: rhei_tui::Slot },
}

struct ParallelAgentSpawned {
    task_id_str: String,
    state_name: String,
    handle: std::thread::JoinHandle<()>,
}

enum ParallelAgentSpawnOutcome {
    Spawned(ParallelAgentSpawned),
    Advanced,
    Skipped,
}

struct ParallelProgramSpawned {
    task_id_str: String,
    state_name: String,
    handle: std::thread::JoinHandle<()>,
}

enum ParallelProgramSpawnOutcome {
    Spawned(ParallelProgramSpawned),
    Skipped,
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

fn format_dry_run_agent_transition(
    task_id: &str,
    from: &str,
    to: &str,
    resolved: &ResolvedAgent,
) -> String {
    let base = format_dry_run_transition(task_id, from, to);
    match resolved_agent_target_slug(resolved) {
        Some(target_slug) => format!("{base} [target={target_slug}]"),
        None => base,
    }
}

fn agent_template_context(resolved: &ResolvedAgent) -> rhei_viz_model::TemplateContext {
    rhei_viz_model::TemplateContext {
        target: resolved.target.as_ref().map(ExecutionTarget::selector),
        target_slug: resolved.target.as_ref().map(ExecutionTarget::slug),
        model: resolved.model.clone(),
        model_provider: resolved.model_provider.clone(),
        model_name: resolved.model_name.clone().or_else(|| resolved.model.clone()),
        agent: Some(resolved.agent.id().to_string()),
        agent_mode: resolved.mode.clone(),
    }
}

fn emit_run_message(
    sink: &Arc<dyn rhei_tui::EventSink>,
    level: rhei_tui::MessageLevel,
    text: impl Into<String>,
) {
    sink.emit(rhei_tui::RunEvent::Message { level, text: text.into() });
}

#[allow(clippy::too_many_arguments)]
fn collect_ready_agent_work_items(
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    active_task_ids: &HashSet<String>,
    active_nonconcurrent_states: &HashSet<String>,
) -> MietteResult<(Vec<AgentWorkItem>, Vec<String>)> {
    let mut agent_tasks = Vec::new();
    let mut state_claimant: HashMap<String, String> = HashMap::new();
    let mut deferred: BTreeSet<String> = BTreeSet::new();

    for task in find_runnable_tasks(&loaded.rhei, machine, workspace_root) {
        let task_id_str = task.id.to_string();
        if active_task_ids.contains(&task_id_str) {
            continue;
        }

        let current_state_raw = task.state.as_str().to_string();
        let current_state = normalized_state_name(&current_state_raw, machine);
        let Some(state_def) = machine.states.get(&current_state) else {
            continue;
        };
        if state_def.program.is_some()
            || state_def.terminal
            || state_def.gating
            || opts.no_agent()
        {
            continue;
        }

        let invocations = resolve_agent_invocations(machine, &current_state, settings, opts)?;
        if invocations.is_empty() {
            if state_declares_autonomous_agent_work(state_def) {
                return Err(miette!("no agent configured for ready state '{}'", current_state));
            }
            continue;
        }

        let pending = if state_def.outputs.is_empty() {
            invocations
        } else {
            invocations
                .into_iter()
                .filter(|resolved| {
                    !state_outputs_exist_for_resolved_invocation(
                        workspace_root,
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
            continue;
        }

        if !opts.dry_run() {
            for resolved in &pending {
                ensure_orchestrator_timeout(resolved, &current_state)?;
            }
        }

        let is_concurrent = state_def.concurrent;
        if !is_concurrent && active_nonconcurrent_states.contains(&current_state) {
            deferred.insert(task_id_str);
            continue;
        }
        if !is_concurrent {
            match state_claimant.get(&current_state) {
                Some(claimant) if claimant == &task_id_str => {}
                Some(_) => {
                    deferred.insert(task_id_str);
                    continue;
                }
                None => {
                    state_claimant.insert(current_state.clone(), task_id_str.clone());
                }
            }
        }

        for resolved in pending {
            agent_tasks.push(AgentWorkItem {
                task_id_str: task_id_str.clone(),
                current_state_raw: current_state_raw.clone(),
                current_state: current_state.clone(),
                resolved,
            });
        }
    }

    Ok((agent_tasks, deferred.into_iter().collect()))
}

#[allow(clippy::too_many_arguments)]
fn collect_ready_program_work_items(
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    active_task_ids: &HashSet<String>,
    active_nonconcurrent_states: &HashSet<String>,
) -> MietteResult<(Vec<ProgramWorkItem>, Vec<String>)> {
    let mut program_tasks = Vec::new();
    let mut state_claimant: HashMap<String, String> = HashMap::new();
    let mut deferred: BTreeSet<String> = BTreeSet::new();

    for task in find_runnable_tasks(&loaded.rhei, machine, workspace_root) {
        let task_id_str = task.id.to_string();
        if active_task_ids.contains(&task_id_str) {
            continue;
        }

        let current_state_raw = task.state.as_str().to_string();
        let current_state = normalized_state_name(&current_state_raw, machine);
        let Some(state_def) = machine.states.get(&current_state) else {
            continue;
        };
        if state_def.program.is_none() || state_def.terminal || state_def.gating || opts.no_program()
        {
            continue;
        }

        let Some(resolved) = resolve_program(machine, &current_state, settings, opts)? else {
            continue;
        };

        let is_concurrent = state_def.concurrent;
        if !is_concurrent && active_nonconcurrent_states.contains(&current_state) {
            deferred.insert(task_id_str);
            continue;
        }
        if !is_concurrent {
            match state_claimant.get(&current_state) {
                Some(claimant) if claimant == &task_id_str => {}
                Some(_) => {
                    deferred.insert(task_id_str);
                    continue;
                }
                None => {
                    state_claimant.insert(current_state.clone(), task_id_str.clone());
                }
            }
        }

        program_tasks.push(ProgramWorkItem {
            task_id_str,
            current_state,
            resolved,
        });
    }

    Ok((program_tasks, deferred.into_iter().collect()))
}

fn take_parallel_slot(free_slots: &mut BTreeSet<rhei_tui::Slot>, next_extra_slot: &mut rhei_tui::Slot) -> rhei_tui::Slot {
    if let Some(slot) = free_slots.pop_first() {
        return slot;
    }
    let slot = *next_extra_slot;
    *next_extra_slot = next_extra_slot.saturating_add(1);
    slot
}

#[allow(clippy::too_many_arguments)]
fn spawn_parallel_agent_work_item(
    item: &AgentWorkItem,
    slot: rhei_tui::Slot,
    tx: std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
) -> MietteResult<ParallelAgentSpawnOutcome> {
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&item.task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(ParallelAgentSpawnOutcome::Skipped) };

    let tooling = resolve_tooling(machine, &item.current_state, settings);
    let gate = gate_tooling_for_agent(&item.resolved, &tooling);
    for warning in &gate.warnings {
        emit_run_message(sink, rhei_tui::MessageLevel::Warn, warning.clone());
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
                &item.task_id_str,
                &item.current_state,
                ToolingKind::Mcp,
                &mcp_unavailable,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => fired = true,
                TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
            }
        }
        if !fired && !skill_unavailable.is_empty() {
            match fire_tooling_unavailable_transition(
                input,
                machine,
                callback_paths,
                &item.task_id_str,
                &item.current_state,
                ToolingKind::Skill,
                &skill_unavailable,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => fired = true,
                TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
            }
        }
        if !fired {
            let message =
                format_required_tooling_error(&item.task_id_str, &item.current_state, &gate.required);
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  error: {message}"));
            if !opts.continue_on_error() {
                return Err(miette!("{message}"));
            }
        }
        return Ok(if fired {
            ParallelAgentSpawnOutcome::Advanced
        } else {
            ParallelAgentSpawnOutcome::Skipped
        });
    }
    let tooling = gate.tooling;
    let checkout_root = resolve_agent_checkout_root(workspace_root, &item.task_id_str)?;
    let render_context = RuntimeTemplateContext {
        workspace_root,
        checkout_root: &checkout_root.path,
        plan_path: &callback_paths.plan_path,
        state_machine_path: callback_paths.state_machine_path.as_deref(),
        plan_title: &loaded.rhei.title,
        task,
        state_name: &item.current_state,
        current_state_raw: task.state.as_str(),
        machine,
        metadata: loaded.rhei.metadata.as_ref(),
        target: item.resolved.target.as_ref(),
        model: item.resolved.model.as_deref(),
        model_provider: item.resolved.model_provider.as_deref(),
        model_name: item.resolved.model_name.as_deref(),
        agent: Some(item.resolved.agent.id()),
        agent_mode: item.resolved.mode.as_deref(),
        tooling: Some(&tooling),
    };
    let prompt = compose_agent_prompt(&render_context)?;
    let visit_count = render_visit_count(
        loaded.rhei.metadata.as_ref(),
        &task.id,
        &item.current_state,
        task.state.as_str(),
        machine,
    );
    let log = agent_log_path(
        runtime_dir,
        &item.task_id_str,
        &item.current_state,
        resolved_agent_log_suffix(&item.resolved, Some(visit_count)).as_deref(),
    );
    let working_dir = checkout_root.path.clone();
    let worktree_root = checkout_root.worktree_root.clone();
    let plan_path = callback_paths.plan_path.clone();
    let state_machine_path = callback_paths.state_machine_path.clone();
    let tid = item.task_id_str.clone();
    let sname = item.current_state.clone();

    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!(
            "\nSpawning agent '{}' for Task {}: {} (parallel)",
            item.resolved.agent.id(),
            item.task_id_str,
            task.title
        ),
    );
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!("  Checkout: {}", working_dir.display()),
    );
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!("  Log: {}", log.display()),
    );

    let snapshot_preload = preload_snapshot_inherit_before_spawn(
        input,
        workspace_root,
        machine,
        task,
        &item.current_state,
        &item.resolved,
        settings,
        visit_count,
        snapshot_override_selection,
        opts,
    )?;

    let from_state = task.state.as_str().to_string();
    let started_at = std::time::Instant::now();
    let started_wall = std::time::SystemTime::now();
    sink.emit(rhei_tui::RunEvent::SlotAssigned {
        slot,
        task: item.task_id_str.clone(),
        from: from_state.clone(),
        to: item.current_state.clone(),
        agent: Some(item.resolved.agent.id().to_string()),
        template_context: Some(agent_template_context(&item.resolved)),
        log_path: log.clone(),
        started_at,
        wall_clock: started_wall,
    });

    let resolved_for_thread = item.resolved.clone();
    let tooling_for_thread = tooling.clone();
    let sink_for_thread = sink.clone();
    let intervene_for_thread = intervene.cloned();
    let log_for_thread = log.clone();
    let log_for_result = log.clone();
    let from_for_thread = from_state;
    let to_for_thread = item.current_state.clone();
    let tid_for_event = item.task_id_str.clone();
    let runtime_dir_for_thread = runtime_dir.to_path_buf();
    let snapshot_preload_for_thread = snapshot_preload.clone();
    let snapshot_preload_for_result = snapshot_preload.clone();
    let visit_for_result = visit_count;
    let resolved_for_result = item.resolved.clone();
    let workspace_root_for_thread = workspace_root.to_path_buf();
    let rhei_root_for_thread = workspace_root.to_path_buf();
    let worktree_root_for_thread = worktree_root.clone();
    let task_for_accounting = task.clone();
    let task_id_for_panic = tid.clone();
    let state_for_panic = sname.clone();

    let handle = std::thread::spawn(move || {
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let resolved = resolved_for_thread;
            let result = spawn_and_wait_agent(
                &resolved,
                &prompt,
                &rhei_root_for_thread,
                &working_dir,
                worktree_root_for_thread.as_deref(),
                &plan_path,
                state_machine_path.as_deref(),
                &tid,
                &sname,
                visit_count,
                &tooling_for_thread,
                &log_for_thread,
                &runtime_dir_for_thread,
                Some(&snapshot_preload_for_thread),
                slot,
                sink_for_thread.clone(),
                intervene_for_thread.as_ref(),
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let (outcome, exit_code) = match &result {
                Ok(outcome) if outcome.status.success() => {
                    (rhei_tui::TaskOutcome::Completed, outcome.status.code())
                }
                Ok(outcome) => {
                    let code = outcome.status.code().unwrap_or(-1);
                    (
                        if outcome.timed_out {
                            rhei_tui::TaskOutcome::TimedOut
                        } else {
                            rhei_tui::TaskOutcome::Failed(format!("exit {code}"))
                        },
                        outcome.status.code(),
                    )
                }
                Err(err) => (rhei_tui::TaskOutcome::Failed(err.to_string()), None),
            };
            let finished_wall = std::time::SystemTime::now();
            sink_for_thread.emit(rhei_tui::RunEvent::SlotReleased {
                slot,
                task: tid_for_event,
                from: from_for_thread,
                to: to_for_thread,
                log_path: log_for_thread.clone(),
                outcome,
                finished_at: std::time::Instant::now(),
                wall_clock: finished_wall,
                exit_code,
                duration_ms,
            });
            let usage_capture_path =
                result.as_ref().ok().and_then(|outcome| outcome.usage_capture_path.as_ref());
            let accounting_result = record_agent_accounting_invocation(AgentAccountingInvocation {
                workspace_root: &workspace_root_for_thread,
                task: &task_for_accounting,
                state: &sname,
                resolved: &resolved,
                visit: visit_count,
                started_at: started_wall,
                ended_at: finished_wall,
                slot: Some(slot),
                usage_capture_path: usage_capture_path.map(PathBuf::as_path),
                log_path: Some(&log_for_thread),
                sink: &sink_for_thread,
            });
            let (accounting_recorded, accounting_warning) = match accounting_result {
                Ok(Some(_)) => (true, None),
                Ok(None) => (false, None),
                Err(err) => (false, Some(err.to_string())),
            };
            ParallelAgentThreadMessage::Completed(ParallelAgentCompletion {
                task_id_str: tid,
                state_name: sname,
                resolved: resolved_for_result,
                log: log_for_result,
                snapshot_preload: snapshot_preload_for_result,
                visit_count: visit_for_result,
                result,
                accounting_recorded,
                accounting_warning,
                slot,
            })
        }));
        let message = thread_result.unwrap_or(ParallelAgentThreadMessage::Panicked {
            task_id_str: task_id_for_panic,
            state_name: state_for_panic,
            slot,
        });
        let _ = tx.send(message);
    });

    Ok(ParallelAgentSpawnOutcome::Spawned(ParallelAgentSpawned {
        task_id_str: item.task_id_str.clone(),
        state_name: item.current_state.clone(),
        handle,
    }))
}

#[allow(clippy::too_many_arguments)]
fn spawn_parallel_program_work_item(
    item: &ProgramWorkItem,
    slot: rhei_tui::Slot,
    tx: std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    workspace_root: &Path,
    runtime_dir: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<ParallelProgramSpawnOutcome> {
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&item.task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(ParallelProgramSpawnOutcome::Skipped) };

    let log = program_log_path(runtime_dir, &item.task_id_str, &item.current_state);
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!("\nSpawning program for Task {}: {} (parallel)", item.task_id_str, task.title),
    );
    emit_run_message(sink, rhei_tui::MessageLevel::Info, format!("  Log: {}", log.display()));

    let from_state = task.state.as_str().to_string();
    let started_at = std::time::Instant::now();
    let started_wall = std::time::SystemTime::now();
    sink.emit(rhei_tui::RunEvent::SlotAssigned {
        slot,
        task: item.task_id_str.clone(),
        from: from_state.clone(),
        to: item.current_state.clone(),
        agent: None,
        template_context: None,
        log_path: log.clone(),
        started_at,
        wall_clock: started_wall,
    });

    let resolved_for_thread = item.resolved.clone();
    let workspace_root_for_thread = workspace_root.to_path_buf();
    let callback_paths_for_thread = callback_paths.clone();
    let plan_title_for_thread = loaded.rhei.title.clone();
    let task_for_thread = task.clone();
    let state_name_for_thread = item.current_state.clone();
    let current_state_raw_for_thread = task.state.as_str().to_string();
    let machine_for_thread = machine.clone();
    let metadata_for_thread = loaded.rhei.metadata.clone();
    let log_for_thread = log.clone();
    let sink_for_thread = sink.clone();
    let task_id_for_result = item.task_id_str.clone();
    let state_name_for_result = item.current_state.clone();
    let task_id_for_panic = item.task_id_str.clone();
    let state_for_panic = item.current_state.clone();

    let handle = std::thread::spawn(move || {
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let render_context = RuntimeTemplateContext {
                workspace_root: &workspace_root_for_thread,
                checkout_root: &workspace_root_for_thread,
                plan_path: &callback_paths_for_thread.plan_path,
                state_machine_path: callback_paths_for_thread.state_machine_path.as_deref(),
                plan_title: &plan_title_for_thread,
                task: &task_for_thread,
                state_name: &state_name_for_thread,
                current_state_raw: &current_state_raw_for_thread,
                machine: &machine_for_thread,
                metadata: metadata_for_thread.as_ref(),
                target: None,
                model: None,
                model_provider: None,
                model_name: None,
                agent: None,
                agent_mode: None,
                tooling: None,
            };
            let result = spawn_and_wait_program(
                &resolved_for_thread,
                &render_context,
                &log_for_thread,
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let (outcome, exit_code) = match &result {
                Ok(program_outcome) if program_outcome.status.success() => {
                    (rhei_tui::TaskOutcome::Completed, program_outcome.status.code())
                }
                Ok(program_outcome) => {
                    let code = program_outcome.status.code().unwrap_or(-1);
                    (
                        if program_outcome.timed_out {
                            rhei_tui::TaskOutcome::TimedOut
                        } else {
                            rhei_tui::TaskOutcome::Failed(format!("exit {code}"))
                        },
                        program_outcome.status.code(),
                    )
                }
                Err(err) => (rhei_tui::TaskOutcome::Failed(err.to_string()), None),
            };
            sink_for_thread.emit(rhei_tui::RunEvent::SlotReleased {
                slot,
                task: task_id_for_result.clone(),
                from: from_state,
                to: state_name_for_result.clone(),
                log_path: log_for_thread,
                outcome,
                finished_at: std::time::Instant::now(),
                wall_clock: std::time::SystemTime::now(),
                exit_code,
                duration_ms,
            });
            ParallelAgentThreadMessage::ProgramCompleted(ParallelProgramCompletion {
                task_id_str: task_id_for_result,
                state_name: state_name_for_result,
                result,
                slot,
            })
        }));
        let message = thread_result.unwrap_or(ParallelAgentThreadMessage::Panicked {
            task_id_str: task_id_for_panic,
            state_name: state_for_panic,
            slot,
        });
        let _ = tx.send(message);
    });

    Ok(ParallelProgramSpawnOutcome::Spawned(ParallelProgramSpawned {
        task_id_str: item.task_id_str.clone(),
        state_name: item.current_state.clone(),
        handle,
    }))
}

struct ParallelScheduleOutcome {
    spawned: usize,
    advanced: bool,
}

#[allow(clippy::too_many_arguments)]
fn schedule_agent_work_items(
    items: Vec<AgentWorkItem>,
    max_new_tasks: usize,
    tx: &std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    let mut selected_task_ids = HashSet::new();
    let mut spawned = 0usize;
    let mut advanced = false;

    for item in items {
        if !selected_task_ids.contains(&item.task_id_str) {
            if selected_task_ids.len() >= max_new_tasks {
                continue;
            }
            selected_task_ids.insert(item.task_id_str.clone());
        }

        let slot = take_parallel_slot(free_slots, next_extra_slot);
        match spawn_parallel_agent_work_item(
            &item,
            slot,
            tx.clone(),
            input,
            machine,
            callback_paths,
            settings,
            opts,
            workspace_root,
            runtime_dir,
            snapshot_override_selection,
            sink,
            intervene,
        )? {
            ParallelAgentSpawnOutcome::Spawned(spawned_agent) => {
                *active_invocation_counts.entry(spawned_agent.task_id_str.clone()).or_insert(0) += 1;
                if !machine
                    .states
                    .get(&spawned_agent.state_name)
                    .map(|state| state.concurrent)
                    .unwrap_or(false)
                {
                    *active_state_counts.entry(spawned_agent.state_name.clone()).or_insert(0) += 1;
                }
                handles.push(spawned_agent.handle);
                spawned += 1;
            }
            ParallelAgentSpawnOutcome::Advanced => {
                free_slots.insert(slot);
                advanced = true;
            }
            ParallelAgentSpawnOutcome::Skipped => {
                free_slots.insert(slot);
            }
        }
    }

    Ok(ParallelScheduleOutcome { spawned, advanced })
}

#[allow(clippy::too_many_arguments)]
fn schedule_program_work_items(
    items: Vec<ProgramWorkItem>,
    max_new_tasks: usize,
    tx: &std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    workspace_root: &Path,
    runtime_dir: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    let mut selected_task_ids = HashSet::new();
    let mut spawned = 0usize;
    let advanced = false;

    for item in items {
        if !selected_task_ids.contains(&item.task_id_str) {
            if selected_task_ids.len() >= max_new_tasks {
                continue;
            }
            selected_task_ids.insert(item.task_id_str.clone());
        }

        let slot = take_parallel_slot(free_slots, next_extra_slot);
        match spawn_parallel_program_work_item(
            &item,
            slot,
            tx.clone(),
            input,
            machine,
            callback_paths,
            workspace_root,
            runtime_dir,
            sink,
        )? {
            ParallelProgramSpawnOutcome::Spawned(spawned_program) => {
                *active_invocation_counts
                    .entry(spawned_program.task_id_str.clone())
                    .or_insert(0) += 1;
                if !machine
                    .states
                    .get(&spawned_program.state_name)
                    .map(|state| state.concurrent)
                    .unwrap_or(false)
                {
                    *active_state_counts.entry(spawned_program.state_name.clone()).or_insert(0) += 1;
                }
                handles.push(spawned_program.handle);
                spawned += 1;
            }
            ParallelProgramSpawnOutcome::Skipped => {
                free_slots.insert(slot);
            }
        }
    }

    Ok(ParallelScheduleOutcome { spawned, advanced })
}

fn release_parallel_worker(
    task_id_str: &str,
    state_name: &str,
    slot: rhei_tui::Slot,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
) {
    free_slots.insert(slot);
    if let Some(count) = active_invocation_counts.get_mut(task_id_str) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            active_invocation_counts.remove(task_id_str);
        }
    }
    if let Some(count) = active_state_counts.get_mut(state_name) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            active_state_counts.remove(state_name);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refill_parallel_worker_pool(
    pass: u32,
    task_limit: usize,
    tx: &std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    // Program and agent work share live capacity.
    // Each completion reloads the ready set. §FS-rhei-run.5 §FS-rhei-programs.6.3
    let task_capacity = if task_limit == usize::MAX {
        usize::MAX
    } else {
        task_limit.saturating_sub(active_invocation_counts.len())
    };
    if task_capacity == 0 {
        return Ok(ParallelScheduleOutcome { spawned: 0, advanced: false });
    }

    let reloaded = load_plan(input)?;
    let active_task_ids = active_invocation_counts.keys().cloned().collect::<HashSet<_>>();
    let active_nonconcurrent_states = active_state_counts.keys().cloned().collect::<HashSet<_>>();
    let (program_items, program_deferred) = collect_ready_program_work_items(
        &reloaded,
        machine,
        settings,
        opts,
        workspace_root,
        &active_task_ids,
        &active_nonconcurrent_states,
    )?;
    if !program_deferred.is_empty() {
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Info,
            format!(
                "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                program_deferred.len(),
                program_deferred.join(", ")
            ),
        );
        sink.emit(rhei_tui::RunEvent::TasksDeferred { pass, tasks: program_deferred });
    }

    let program_outcome = schedule_program_work_items(
        program_items,
        task_capacity,
        tx,
        input,
        machine,
        callback_paths,
        workspace_root,
        runtime_dir,
        sink,
        free_slots,
        next_extra_slot,
        active_invocation_counts,
        active_state_counts,
        handles,
    )?;

    let task_capacity = if task_limit == usize::MAX {
        usize::MAX
    } else {
        task_limit.saturating_sub(active_invocation_counts.len())
    };
    if task_capacity == 0 {
        return Ok(program_outcome);
    }

    let reloaded = load_plan(input)?;
    let active_task_ids = active_invocation_counts.keys().cloned().collect::<HashSet<_>>();
    let active_nonconcurrent_states = active_state_counts.keys().cloned().collect::<HashSet<_>>();
    let (agent_items, agent_deferred) = collect_ready_agent_work_items(
        &reloaded,
        machine,
        settings,
        opts,
        workspace_root,
        &active_task_ids,
        &active_nonconcurrent_states,
    )?;
    if !agent_deferred.is_empty() {
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Info,
            format!(
                "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                agent_deferred.len(),
                agent_deferred.join(", ")
            ),
        );
        sink.emit(rhei_tui::RunEvent::TasksDeferred { pass, tasks: agent_deferred });
    }

    let refill_candidates = agent_items
        .iter()
        .map(|item| {
            (
                item.task_id_str.clone(),
                item.current_state_raw.clone(),
                item.current_state.clone(),
                item.resolved.clone(),
            )
        })
        .collect::<Vec<_>>();
    let snapshot_override_selection =
        select_snapshot_override_run_invocation(machine, opts, &refill_candidates)?;
    let agent_outcome = schedule_agent_work_items(
        agent_items,
        task_capacity,
        tx,
        input,
        machine,
        callback_paths,
        settings,
        opts,
        workspace_root,
        runtime_dir,
        snapshot_override_selection.as_ref(),
        sink,
        intervene,
        free_slots,
        next_extra_slot,
        active_invocation_counts,
        active_state_counts,
        handles,
    )?;

    Ok(ParallelScheduleOutcome {
        spawned: program_outcome.spawned + agent_outcome.spawned,
        advanced: program_outcome.advanced || agent_outcome.advanced,
    })
}

struct ParallelProgramCompletionEffect {
    advanced: bool,
    program_spawned: bool,
}

fn handle_parallel_program_completion(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    opts: &RunOptions,
    workspace_root: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    completion: ParallelProgramCompletion,
) -> MietteResult<ParallelProgramCompletionEffect> {
    let ParallelProgramCompletion {
        task_id_str,
        state_name,
        result,
        slot: _,
    } = completion;

    match result {
        Ok(program_outcome) => {
            let mut advanced = false;
            let target_id = parse_task_id(&task_id_str);
            let mut reloaded = load_plan(input)?;
            let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
            let mut state_after =
                task_after.map(|task| task.state.as_str()).unwrap_or("unknown").to_string();

            if normalized_state_name(&state_after, machine)
                != normalized_state_name(&state_name, machine)
            {
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Info,
                    format!(
                        "  Task {} advanced: '{}' -> '{}'",
                        task_id_str, state_name, state_after
                    ),
                );
                return Ok(ParallelProgramCompletionEffect {
                    advanced: true,
                    program_spawned: true,
                });
            }

            if program_outcome.timed_out {
                match fire_timeout_transition(
                    input,
                    machine,
                    callback_paths,
                    &task_id_str,
                    &state_name,
                    program_outcome.timeout_secs,
                    opts.no_callbacks(),
                ) {
                    TimeoutTransitionOutcome::Fired => {}
                    TimeoutTransitionOutcome::NoRule => {
                        emit_run_message(
                            sink,
                            rhei_tui::MessageLevel::Warn,
                            format!(
                                "  warning: program for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                                task_id_str, state_name
                            ),
                        );
                    }
                    TimeoutTransitionOutcome::Failed => {}
                }
                reloaded = load_plan(input)?;
                state_after = reloaded
                    .rhei
                    .tasks
                    .iter()
                    .find(|task| task.id == target_id)
                    .map(|task| task.state.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                if normalized_state_name(&state_after, machine)
                    != normalized_state_name(&state_name, machine)
                {
                    emit_run_message(
                        sink,
                        rhei_tui::MessageLevel::Info,
                        format!(
                            "  Task {} advanced: '{}' -> '{}'",
                            task_id_str, state_name, state_after
                        ),
                    );
                    advanced = true;
                }
                return Ok(ParallelProgramCompletionEffect {
                    advanced,
                    program_spawned: true,
                });
            }

            let exit_code = program_outcome.status.code().unwrap_or(-1);
            let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
            let Some(task) = task_after else {
                return Ok(ParallelProgramCompletionEffect {
                    advanced,
                    program_spawned: true,
                });
            };

            if let Some(to_state) = find_program_exit_transition(
                machine,
                reloaded.rhei.metadata.as_ref(),
                task,
                &state_name,
                exit_code,
            )? {
                if exit_code == 0 && to_state != state_name {
                    let missing_required_outputs = collect_missing_required_outputs(
                        workspace_root,
                        machine,
                        reloaded.rhei.metadata.as_ref(),
                        task,
                        &state_name,
                    );
                    if !missing_required_outputs.is_empty() {
                        emit_run_message(
                            sink,
                            rhei_tui::MessageLevel::Warn,
                            format!(
                                "  warning: program exited 0 but required outputs are missing for task {} in state '{}': {}",
                                task_id_str,
                                state_name,
                                missing_required_outputs.join(", ")
                            ),
                        );
                        return Ok(ParallelProgramCompletionEffect {
                            advanced,
                            program_spawned: true,
                        });
                    }
                }
                if record_poll_self_loop_if_needed(
                    input,
                    reloaded.rhei.metadata.as_ref(),
                    machine,
                    task,
                    &state_name,
                    &to_state,
                )? {
                    emit_run_message(
                        sink,
                        rhei_tui::MessageLevel::Info,
                        format!(
                            "  Task {} poll self-loop scheduled next attempt from '{}'",
                            task_id_str, state_name
                        ),
                    );
                    return Ok(ParallelProgramCompletionEffect {
                        advanced: true,
                        program_spawned: true,
                    });
                }
                let task_file = reloaded.task_file(&task_id_str, input);
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
                    &task_id_str,
                    &state_name,
                    &to_state,
                    exit_code,
                    opts.no_callbacks(),
                )?;
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Info,
                    format!(
                        "  Task {} advanced: '{}' -> '{}'",
                        task_id_str, state_name, to_state
                    ),
                );
                advanced = true;
            } else if program_outcome.status.success() {
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Warn,
                    format!(
                        "  warning: program exited 0 but task {} did not advance from '{}'",
                        task_id_str, state_name
                    ),
                );
            } else {
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Error,
                    format!(
                        "  error: program exited with code {} for task {}",
                        exit_code, task_id_str
                    ),
                );
                if !opts.continue_on_error() {
                    return Err(miette!(
                        "program exited with code {} for Task {}. Use --continue-on-error to skip failures.",
                        exit_code,
                        task_id_str
                    ));
                }
            }

            Ok(ParallelProgramCompletionEffect {
                advanced,
                program_spawned: true,
            })
        }
        Err(err) => {
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  error: {}", err));
            if !opts.continue_on_error() {
                return Err(err);
            }
            Ok(ParallelProgramCompletionEffect {
                advanced: false,
                program_spawned: false,
            })
        }
    }
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
    // §FS-rhei-run-report.3.1: run duration shown in the end-of-run summary.
    let run_started = TuiInstant::now();
    // §FS-rhei-run-report.2: wall-clock start and run id for the durable report.
    let run_started_wall = SystemTime::now();
    let run_id = short_run_id(run_started_wall);

    let command = current_command_line();

    let (initial_total_tasks, initial_states) = {
        let loaded = load_plan(input)?;
        (total_task_count(&loaded.rhei), collect_initial_states(&loaded.rhei, machine))
    };
    // §FS-rhei-run-report.1: declared before the frontend so it drops *after* the
    // terminal is restored; the happy path disarms it once the full report is
    // written, so it only fires when the run returns early with an error.
    let mut report_guard = RunReportGuard {
        input,
        machine,
        runtime_dir: runtime_dir.clone(),
        run_started,
        run_started_wall,
        run_id: run_id.clone(),
        workspace_root: workspace_root.clone(),
        command: command.clone(),
        parallel: max_parallel,
        mode: "agent",
        initial_states: initial_states.clone(),
        dry_run: opts.dry_run(),
        summary: None,
        armed: true,
    };
    let frontend_parallel = max_parallel.max(1).min(u16::MAX as usize) as u16;
    let frontend = start_run_frontend(
        &workspace_root,
        input,
        callback_paths,
        opts,
        frontend_parallel,
        initial_total_tasks,
        machine,
    );
    let sink = frontend.sink.clone();
    // Route leaf-helper diagnostics through the frontend for the run's duration
    // instead of letting them write straight to the terminal and corrupt the
    // TUI. §FS-rhei-run-tui.1.8
    let diag_guard = RunDiagGuard::install(sink.clone());
    // Held past the frontend drop so the end-of-run summary can read per-task
    // activity after the TUI restores the terminal. §FS-rhei-run-report.3
    let summary_sink = frontend.summary.clone();
    report_guard.summary = Some(summary_sink.clone());
    let dashboard_enabled = frontend.dashboard.is_some();
    // AR §7: present only when the dashboard is live; each spawned agent's stdin
    // is registered here so `/intervene` can stream messages to it.
    let intervene = frontend.intervene.clone();
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
    let initial_terminal_count = terminal_task_count(&loaded.rhei, machine);
    run_info!(
        "Running {} '{}' with {} task(s) ({} terminal at start).",
        if workspace::is_workspace(input) { "workspace" } else { "plan" },
        loaded.rhei.title,
        total_task_count(&loaded.rhei),
        initial_terminal_count
    );
    run_info!("Initial states: {}", format_state_counts(&loaded.rhei));

    let mut agents_spawned = 0u32;
    let mut programs_spawned = 0u32;
    let mut callback_transitions_made = 0u32;
    let mut pass = 0u32;
    // One-time notice so the gate-wait below does not spam the journal each tick.
    let mut awaiting_gate_announced = false;

    loop {
        let loaded = load_plan(input)?;
        let ready = find_runnable_tasks(&loaded.rhei, machine, &workspace_root);
        if ready.is_empty() {
            if !opts.dry_run() {
                // Interactive TUI: stay alive only when human gates are the
                // remaining blocker, so unrelated stuck work still reaches the
                // normal halt/error path. §FS-rhei-run-tui.1.5.5
                if frontend.is_tui && should_wait_for_human_gate(&loaded.rhei, machine) {
                    if !awaiting_gate_announced {
                        run_info!(
                            "Waiting for human gate decisions — resolve a gate in the UI, or press Ctrl+C to stop."
                        );
                        awaiting_gate_announced = true;
                    }
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
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
        // Made progress this pass; re-arm the gate-wait notice for any later gate.
        awaiting_gate_announced = false;

        pass += 1;
        let terminal_count = terminal_task_count(&loaded.rhei, machine);
        sink.emit(RunEvent::PassStarted {
            pass,
            ready: ready.iter().map(|t| t.id.to_string()).collect(),
        });
        run_info!(
            "\nPass {}: {} ready, {} terminal, {} total.",
            pass,
            ready.len(),
            terminal_count,
            total_task_count(&loaded.rhei)
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
                let invocations = resolve_agent_invocations_for_task(
                    machine,
                    &current_state,
                    settings,
                    opts,
                    Some(task),
                )?;
                if invocations.is_empty() {
                    if opts.no_agent() {
                        callback_tasks.push((task_id_str, current_state_raw, current_state));
                        continue;
                    }
                    // Surface every remediation slot from the resolution order:
                    // `defaults.agent`, the state's
                    // `agent`, `models.<id>.default_agent`, and `--agent`.
                    // Mention the resolved model id when one is set so
                    // operators can locate `models.<id>.default_agent`.

                    // §FS-rhei-agents.1.4: Explain unresolved agent slots.
                    let resolved_model = state_def
                        .model
                        .clone()
                        .or_else(|| settings.defaults.model.clone())
                        .or_else(|| settings.model.clone());
                    let model_remediation = match &resolved_model {
                        Some(id) => format!(
                            "models.{id}.default_agent in {}/{}",
                            workspace_root.display(),
                            PROJECT_SETTINGS_RELATIVE_PATH
                        ),
                        None => "models.<id>.default_agent (in settings.json)".to_string(),
                    };
                    let header = match &resolved_model {
                        Some(id) => format!("no agent configured for model '{id}'."),
                        None => "no agent configured.".to_string(),
                    };
                    return Err(miette!(
                        "{header}\n\nSet one of:\n  \u{2022} defaults.agent in {}/{} or ~/.config/rhei/settings.json\n  \u{2022} the state's `agent:` in states.yaml\n  \u{2022} {model_remediation}\n  \u{2022} --agent <AGENT> on the rhei run command line (e.g. rhei run {} --agent claude-code)\n\nBuilt-in agents: claude-code, codex, gemini, cursor, kilocode, pi",
                        workspace_root.display(),
                        PROJECT_SETTINGS_RELATIVE_PATH,
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
                // out above and do not need a timeout.

                // §FS-rhei-agents.3.1 §FS-rhei-agents.3.2: Require timeout.
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
        let run_programs_in_worker_pool = max_parallel != 1;

        // Handle callback-only tasks first (fast, synchronous).
        for (task_id_str, current_state_raw, current_state) in &callback_tasks {
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let task = match find_task_by_id(&loaded.rhei.tasks, &target_id) {
                Some(t) => t,
                None => continue,
            };
            if let Some(to_state) = manual_initial_terminal_transition(task, &loaded.rhei, machine)? {
                return Err(miette!(
                    "Task {} is in manual-only initial state '{}' with terminal transition to '{}'; \
                     use `rhei next`, do the task, then `rhei complete` instead of `rhei run`.",
                    task_id_str,
                    current_state,
                    to_state
                ));
            }
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
                Ok(effective_to) => {
                    let root = result_workspace_root(input, &task_file);
                    record_transition_result(
                        &root,
                        &task_file,
                        machine,
                        task_id_str,
                        current_state,
                        &effective_to,
                        None,
                    )?;
                    run_info!(
                        "Task {} transitioned: '{}' \u{2192} '{}'",
                        task_id_str,
                        current_state_raw,
                        effective_to
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

        if !program_tasks.is_empty() && !run_programs_in_worker_pool {
            if opts.dry_run() {
                for (task_id_str, current_state_raw, current_state, resolved) in &program_tasks {
                    let loaded = load_plan(input)?;
                    let target_id = parse_task_id(task_id_str);
                    if let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) {
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
                let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
                let Some(task) = task else { continue };
                let render_context = RuntimeTemplateContext {
                    workspace_root: &workspace_root,
                    checkout_root: &workspace_root,
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

    let started_at = std::time::Instant::now();
    let started_wall = std::time::SystemTime::now();
                sink.emit(RunEvent::SlotAssigned {
                    slot: 0,
                    task: task_id_str.clone(),
                    from: task.state.as_str().to_string(),
                    to: current_state.clone(),
                    agent: None,
                    template_context: None,
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
                        let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
                        let mut state_after =
                            task_after.map(|t| t.state.as_str()).unwrap_or("unknown").to_string();

                        if normalized_state_name(&state_after, machine)
                            != normalized_state_name(current_state, machine)
                        {
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
                            if normalized_state_name(&state_after, machine)
                                != normalized_state_name(current_state, machine)
                            {
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

        if agent_tasks.is_empty() && (program_tasks.is_empty() || !run_programs_in_worker_pool) {
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
            if run_programs_in_worker_pool {
                for (task_id_str, current_state_raw, current_state, resolved) in &program_tasks {
                    let loaded = load_plan(input)?;
                    let target_id = parse_task_id(task_id_str);
                    if let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) {
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
                                    &to_state,
                                )
                            );
                        }
                    }
                    let _ = resolved;
                }
            }
            for (task_id_str, current_state_raw, current_state, resolved) in &batch {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                if let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) {
                    if let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? {
                        run_info!(
                            "{}",
                            format_dry_run_agent_transition(
                                task_id_str,
                                current_state_raw,
                                &to_state,
                                resolved,
                            )
                        );
                    }
                }
                let _ = current_state;
            }
            sink.emit(RunEvent::PassEnded { pass, progressed: false });
            break;
        }

        // Spawn agents (sequential or parallel).
        if batch_size == 1 && (program_tasks.is_empty() || !run_programs_in_worker_pool) {
            // Sequential: spawn one agent at a time.
            let (task_id_str, _current_state_raw, current_state, resolved) = &batch[0];
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
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
            let checkout_root = resolve_agent_checkout_root(&workspace_root, task_id_str)?;
            let render_context = RuntimeTemplateContext {
                workspace_root: &workspace_root,
                checkout_root: &checkout_root.path,
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
            let prompt = compose_agent_prompt(&render_context)?;
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
            run_info!("  Checkout: {}", checkout_root.path.display());
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
                template_context: Some(agent_template_context(resolved)),
                log_path: log.clone(),
                started_at,
                wall_clock: started_wall,
            });

            let spawn_result = spawn_and_wait_agent(
                resolved,
                &prompt,
                &workspace_root,
                &checkout_root.path,
                checkout_root.worktree_root.as_deref(),
                &callback_paths.plan_path,
                callback_paths.state_machine_path.as_deref(),
                task_id_str,
                current_state,
                visit_count,
                &tooling,
                &log,
                &runtime_dir,
                Some(&snapshot_preload),
                0,
                sink.clone(),
                intervene.as_ref(),
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
            // §FS-rhei-cost-accounting.4: Extraction happens after agent exit.
            match record_agent_accounting_invocation(AgentAccountingInvocation {
                workspace_root: &workspace_root,
                task,
                state: current_state,
                resolved,
                visit: visit_count,
                started_at: started_wall,
                ended_at: finished_wall,
                slot: Some(0),
                usage_capture_path: spawn_result
                    .as_ref()
                    .ok()
                    .and_then(|outcome| outcome.usage_capture_path.as_deref()),
                log_path: Some(&log),
                sink: &sink,
            }) {
                Ok(Some(_)) => {
                    if let Err(err) = regenerate_accounting_indexes(&workspace_root, &loaded.rhei)
                    {
                        run_warn!("  warning: failed to update accounting rollups: {}", err);
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    run_warn!("  warning: failed to record accounting: {}", err);
                }
            }

            match spawn_result {
                Ok(AgentSpawnOutcome { status, timed_out, timeout_secs, .. }) => {
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
                    let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
                    let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
                    let state_before = current_state.as_str();

                    // Compare normalized state names: a counted state and its
                    // visit-suffixed form (e.g. `build` vs `build-2`) are the
                    // same logical state. Comparing raw vs. normalized would
                    // mistake a no-op re-entry for forward progress and skip
                    // the real auto-advance, spinning the loop forever.
                    if normalized_state_name(state_after, machine)
                        != normalized_state_name(state_before, machine)
                    {
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
            // Parallel worker pool: each worker reports completion over a
            // channel, and the scheduler refills freed capacity after every
            // processed result. Re-reading the plan preserves dependency checks. §FS-rhei-run.5
            let (tx, rx) = std::sync::mpsc::channel::<ParallelAgentThreadMessage>();
            let mut handles = Vec::new();
            let mut free_slots: BTreeSet<rhei_tui::Slot> = (0..frontend_parallel).collect();
            let mut next_extra_slot = frontend_parallel;
            let mut active_invocation_counts: HashMap<String, usize> = HashMap::new();
            let mut active_state_counts: HashMap<String, usize> = HashMap::new();
            let initial_program_items = if run_programs_in_worker_pool {
                program_tasks
                    .iter()
                    .map(|(task_id_str, _current_state_raw, current_state, resolved)| ProgramWorkItem {
                        task_id_str: task_id_str.clone(),
                        current_state: current_state.clone(),
                        resolved: resolved.clone(),
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let program_schedule_outcome = schedule_program_work_items(
                initial_program_items,
                task_limit,
                &tx,
                input,
                machine,
                callback_paths,
                &workspace_root,
                &runtime_dir,
                &sink,
                &mut free_slots,
                &mut next_extra_slot,
                &mut active_invocation_counts,
                &mut active_state_counts,
                &mut handles,
            )?;
            advanced_any |= program_schedule_outcome.advanced;

            let agent_capacity = if task_limit == usize::MAX {
                usize::MAX
            } else {
                task_limit.saturating_sub(active_invocation_counts.len())
            };
            let initial_items = batch
                .iter()
                .map(|(task_id_str, current_state_raw, current_state, resolved)| AgentWorkItem {
                    task_id_str: task_id_str.clone(),
                    current_state_raw: current_state_raw.clone(),
                    current_state: current_state.clone(),
                    resolved: resolved.clone(),
                })
                .collect::<Vec<_>>();
            let schedule_outcome = schedule_agent_work_items(
                initial_items,
                agent_capacity,
                &tx,
                input,
                machine,
                callback_paths,
                settings,
                opts,
                &workspace_root,
                &runtime_dir,
                snapshot_override_selection.as_ref(),
                &sink,
                intervene.as_ref(),
                &mut free_slots,
                &mut next_extra_slot,
                &mut active_invocation_counts,
                &mut active_state_counts,
                &mut handles,
            )?;
            advanced_any |= schedule_outcome.advanced;
            let mut active_worker_count =
                program_schedule_outcome.spawned + schedule_outcome.spawned;

            while active_worker_count > 0 {
                let completion = match rx.recv() {
                    Ok(ParallelAgentThreadMessage::Completed(completion)) => completion,
                    Ok(ParallelAgentThreadMessage::ProgramCompleted(completion)) => {
                        active_worker_count = active_worker_count.saturating_sub(1);
                        release_parallel_worker(
                            &completion.task_id_str,
                            &completion.state_name,
                            completion.slot,
                            &mut free_slots,
                            &mut active_invocation_counts,
                            &mut active_state_counts,
                        );
                        let effect = handle_parallel_program_completion(
                            input,
                            machine,
                            callback_paths,
                            opts,
                            &workspace_root,
                            &sink,
                            completion,
                        )?;
                        if effect.program_spawned {
                            programs_spawned += 1;
                        }
                        advanced_any |= effect.advanced;
                        let refill_outcome = refill_parallel_worker_pool(
                            pass,
                            task_limit,
                            &tx,
                            input,
                            machine,
                            callback_paths,
                            settings,
                            opts,
                            &workspace_root,
                            &runtime_dir,
                            &sink,
                            intervene.as_ref(),
                            &mut free_slots,
                            &mut next_extra_slot,
                            &mut active_invocation_counts,
                            &mut active_state_counts,
                            &mut handles,
                        )?;
                        active_worker_count += refill_outcome.spawned;
                        advanced_any |= refill_outcome.advanced;
                        continue;
                    }
                    Ok(ParallelAgentThreadMessage::Panicked {
                        task_id_str,
                        state_name,
                        slot,
                    }) => {
                        active_worker_count = active_worker_count.saturating_sub(1);
                        release_parallel_worker(
                            &task_id_str,
                            &state_name,
                            slot,
                            &mut free_slots,
                            &mut active_invocation_counts,
                            &mut active_state_counts,
                        );
                        let err = miette!("agent thread panicked");
                        run_error!("  error for task {}: {}", task_id_str, err);
                        if !opts.continue_on_error() {
                            return Err(err);
                        }
                        continue;
                    }
                    Err(_) => break,
                };

                active_worker_count = active_worker_count.saturating_sub(1);
                release_parallel_worker(
                    &completion.task_id_str,
                    &completion.state_name,
                    completion.slot,
                    &mut free_slots,
                    &mut active_invocation_counts,
                    &mut active_state_counts,
                );

                let ParallelAgentCompletion {
                    task_id_str,
                    state_name,
                    resolved,
                    log,
                    snapshot_preload,
                    visit_count,
                    result,
                    accounting_recorded,
                    accounting_warning,
                    slot: _,
                } = completion;
                // §FS-rhei-cost-accounting.11: Parallel accounting failures still warn.
                if let Some(warning) = accounting_warning {
                    run_warn!("  warning: failed to record accounting: {}", warning);
                }
                match result {
                    Ok(AgentSpawnOutcome { status, timed_out, timeout_secs, .. }) => {
                        agents_spawned += 1;
                        let target_id = parse_task_id(&task_id_str);
                        let reloaded = load_plan(input)?;
                        if accounting_recorded {
                            if let Err(err) =
                                regenerate_accounting_indexes(&workspace_root, &reloaded.rhei)
                            {
                                run_warn!(
                                    "  warning: failed to update accounting rollups: {}",
                                    err
                                );
                            }
                        }
                        let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
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
                        if normalized_state_name(state_after, machine)
                            != normalized_state_name(&state_name, machine)
                        {
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
                                        find_task_by_id(&reloaded.rhei.tasks, &target_id)
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
                        if accounting_recorded {
                            let reloaded = load_plan(input)?;
                            if let Err(rollup_err) =
                                regenerate_accounting_indexes(&workspace_root, &reloaded.rhei)
                            {
                                run_warn!(
                                    "  warning: failed to update accounting rollups: {}",
                                    rollup_err
                                );
                            }
                        }
                        run_error!("  error for task {}: {}", task_id_str, err);
                        if !opts.continue_on_error() {
                            return Err(err);
                        }
                    }
                }

                let refill_outcome = refill_parallel_worker_pool(
                    pass,
                    task_limit,
                    &tx,
                    input,
                    machine,
                    callback_paths,
                    settings,
                    opts,
                    &workspace_root,
                    &runtime_dir,
                    &sink,
                    intervene.as_ref(),
                    &mut free_slots,
                    &mut next_extra_slot,
                    &mut active_invocation_counts,
                    &mut active_state_counts,
                    &mut handles,
                )?;
                active_worker_count += refill_outcome.spawned;
                advanced_any |= refill_outcome.advanced;
            }

            for handle in handles {
                if handle.join().is_err() {
                    let err = miette!("agent thread panicked");
                    run_error!("  error: {}", err);
                    if !opts.continue_on_error() {
                        return Err(err);
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
            let terminal_count = terminal_task_count(&loaded.rhei, machine);
            let total_tasks = total_task_count(&loaded.rhei);
            run_info!(
                "\nRun complete: {} callback transition(s), {}/{} tasks in terminal state.",
                callback_transitions_made,
                terminal_count,
                total_tasks
            );
            run_info!("Final states: {}", format_state_counts(&loaded.rhei));
            let mut tasks = Vec::new();
            collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
            for task in tasks {
                run_info!("  - {} [{}]", format_task_label(task), task.state);
            }
            (terminal_count, total_tasks)
        }
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = terminal_task_count(&loaded.rhei, machine);
        let total_tasks = total_task_count(&loaded.rhei);
        run_info!(
            "\nRun complete: {} agent(s), {} program(s) spawned, {}/{} tasks in terminal state.",
            agents_spawned,
            programs_spawned,
            terminal_count,
            total_tasks
        );
        run_info!("Final states: {}", format_state_counts(&loaded.rhei));
        let mut tasks = Vec::new();
        collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
        for task in tasks {
            run_info!("  - {} [{}]", format_task_label(task), task.state);
        }
        (terminal_count, total_tasks)
    };

    let accounting = if opts.dry_run() {
        None
    } else {
        // §FS-rhei-cost-accounting.7: RunFinished carries available run totals.
        match load_plan(input) {
            Ok(loaded) => match regenerate_accounting_indexes(&workspace_root, &loaded.rhei) {
                Ok(summary) => summary,
                Err(err) => {
                    run_warn!("  warning: failed to finalize accounting rollups: {}", err);
                    None
                }
            },
            Err(_) => None,
        }
    };

    sink.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned,
            programs_spawned,
            terminal_tasks: terminal_count,
            total_tasks,
            accounting,
        },
    });
    frontend.write_frozen_dashboard();
    drop(diag_guard);
    drop(sink);
    drop(frontend);

    // §FS-rhei-run-report.1/.3: write the durable report (skipped under --dry-run,
    // §3.5), print the console summary or `Report:` pointer (§3.4), then disarm the
    // guard so its fallback only fires on an early error.
    emit_run_report(
        input,
        machine,
        &summary_sink,
        &runtime_dir,
        RunStats {
            agents_spawned,
            programs_spawned,
            callback_only: callback_transitions_made,
            duration: Some(run_started.elapsed()),
            dashboard: frozen_dashboard_relative_path(
                dashboard_enabled,
                &runtime_dir,
                &workspace_root,
            ),
            run_id,
            started_at: Some(run_started_wall),
            workspace_root: workspace_root.clone(),
            command,
            parallel: max_parallel,
            mode: "agent",
            initial_states,
            dry_run: opts.dry_run(),
        },
    );
    report_guard.disarm();

    if !opts.dry_run() {
        let loaded = load_plan(input)?;
        let terminal_count = terminal_task_count(&loaded.rhei, machine);
        if terminal_count < total_task_count(&loaded.rhei)
            && !remaining_work_is_only_gating_or_poll_blocked(&loaded.rhei, machine)
        {
            return Err(miette!(
                "rhei run halted with non-terminal tasks remaining and no further advancement possible"
            ));
        }
    }

    Ok(())
}
