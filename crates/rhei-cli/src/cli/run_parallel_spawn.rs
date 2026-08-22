// Putting one work item on a worker thread: the slot it takes, the prompt and
// snapshot staging it needs before the process starts, and the completion it
// sends back down the channel.
//
// Its own part because spawning is where an invocation stops being schedulable
// data and becomes a live subprocess; the scheduler next door only decides
// which items get this far, and how many at a time.

// §AR-source-file-size.3 §FS-rhei-run.3

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
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
) -> MietteResult<ParallelAgentSpawnOutcome> {
    // The work item was chosen before the interrupt arrived; starting it now
    // would be new work the shutdown promised not to schedule.
    // §FS-rhei-run.3.2
    if interrupt_requested() {
        return Ok(ParallelAgentSpawnOutcome::Skipped);
    }
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&item.task_id_str);
    // The item's owning rhei supplies its machine and callback base.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&item.task_id_str);
    let callback_paths = machines.callbacks_for_str(&item.task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(ParallelAgentSpawnOutcome::Skipped) };

    // Attribute the spawned unit to its owning rhei: prompts, logs, and
    // artifacts resolve against that rhei's execution root. §FS-rhei-panta.6.2
    let task_workspace_root = loaded.task_root(&item.task_id_str, workspace_root);
    let workspace_root = task_workspace_root.as_path();

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
                machines,
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
                machines,
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
                return Err(miette!(
                    help = run_report_help(),
                    "{message}"
                ));
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
        task_roots: Some(&loaded.task_roots),
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
    // Failing the pass would take every healthy sibling down with it.
    // §FS-rhei-run.3: an uncomposable prompt fails its task, not the run.
    let prompt = match compose_agent_prompt(&render_context) {
        Ok(prompt) => prompt,
        Err(err) => {
            let message = format!("Task {} cannot be prompted: {err}", item.task_id_str);
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  error: {message}"));
            if !opts.continue_on_error() {
                return Err(err);
            }
            return Ok(ParallelAgentSpawnOutcome::Unpromptable(item.task_id_str.clone()));
        }
    };
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
    // A fanned-out invocation writes its own result fragment. §FS-rhei-states.3.3
    let result_identity = fanout_result_identity(
        machine.states.get(item.current_state.as_str()),
        item.resolved.target.as_ref(),
        item.resolved.model.as_deref(),
    );

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
    // The worker spawns this run's subprocess, so the run's shutdown guard —
    // and no other — owns the group it leads. §FS-rhei-run.3.2
    let run_owner = current_run_owner();

    let handle = std::thread::spawn(move || {
        inherit_run_owner(run_owner);
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
                result_identity.as_deref(),
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let (outcome, exit_code) = slot_outcome(&result);
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
    machines: &ExecutionMachines,
    workspace_root: &Path,
    runtime_dir: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<ParallelProgramSpawnOutcome> {
    // As for agents: a slot was reserved for this item before the interrupt,
    // and the shutdown starts nothing further. §FS-rhei-run.3.2
    if interrupt_requested() {
        return Ok(ParallelProgramSpawnOutcome::Skipped);
    }
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&item.task_id_str);
    // The item's owning rhei supplies its machine and callback base.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&item.task_id_str);
    let callback_paths = machines.callbacks_for_str(&item.task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(ParallelProgramSpawnOutcome::Skipped) };

    // Programs run against the owning rhei's execution root. §FS-rhei-panta.6.2
    let task_workspace_root = loaded.task_root(&item.task_id_str, workspace_root);
    let workspace_root = task_workspace_root.as_path();

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
    let task_roots_for_thread = loaded.task_roots.clone();
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
    // §FS-rhei-run.3.2: the program's group belongs to this run.
    let run_owner = current_run_owner();

    let handle = std::thread::spawn(move || {
        inherit_run_owner(run_owner);
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let render_context = RuntimeTemplateContext {
                workspace_root: &workspace_root_for_thread,
                task_roots: Some(&task_roots_for_thread),
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
                &sink_for_thread,
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let (outcome, exit_code) = slot_outcome(&result);
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
