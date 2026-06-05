
/// Callback-only execution mode (legacy behavior, used with --no-agent).
fn run_callback_mode(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    opts: &RunOptions,
    max_parallel: usize,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent, RunSummary};

    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let runtime_dir = workspace_root.join("runtime");
    // §FS-rhei-run-report.3.1: run duration shown in the end-of-run summary.
    let run_started = std::time::Instant::now();
    // §FS-rhei-run-report.2: wall-clock start and run id for the durable report.
    let run_started_wall = std::time::SystemTime::now();
    let run_id = short_run_id(run_started_wall);
    let command = current_command_line();
    let initial = load_plan(input)?;
    let initial_total_tasks = total_task_count(&initial.rhei);
    let initial_states = collect_initial_states(&initial.rhei, machine);
    // §FS-rhei-run-report.1: declared before the frontend so it drops after the
    // terminal is restored; disarmed on the happy path (see end of run).
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
        mode: "callback",
        initial_states: initial_states.clone(),
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
    // Held past the frontend drop so the end-of-run summary can read activity
    // after the TUI restores the terminal. §FS-rhei-run-report.3
    let summary_sink = frontend.summary.clone();
    report_guard.summary = Some(summary_sink.clone());
    let dashboard_enabled = frontend.dashboard.is_some();
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

    let initial_terminal_count = terminal_task_count(&initial.rhei, machine);
    run_info!(
        "Running {} '{}' with {} task(s) ({} terminal at start).",
        if workspace::is_workspace(input) { "workspace" } else { "plan" },
        initial.rhei.title,
        initial_total_tasks,
        initial_terminal_count
    );
    run_info!("Initial states: {}", format_state_counts(&initial.rhei));

    let mut transitions_made = 0u32;
    let mut pass = 0u32;
    let mut visited_ready_states = BTreeSet::<(String, String)>::new();

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
        let terminal_count = terminal_task_count(&loaded.rhei, machine);
        sink.emit(RunEvent::PassStarted {
            pass,
            ready: ready.iter().map(|task| task.id.to_string()).collect(),
        });
        run_info!(
            "\nPass {}: {} ready, {} terminal, {} total.",
            pass,
            ready.len(),
            terminal_count,
            total_task_count(&loaded.rhei)
        );
        run_info!("Ready: {}", format_ready_tasks(&ready));

        let mut advanced_any = false;
        let mut stalled_ready_tasks = Vec::new();

        for task in &ready {
            let task_id_str = task.id.to_string();
            let current_state_raw = task.state.as_str();
            let current_state = normalized_state_name(current_state_raw, machine);
            let visit_key = (task_id_str.clone(), current_state_raw.to_string());
            if visited_ready_states.contains(&visit_key) {
                stalled_ready_tasks.push(format!(
                    "{} (already visited '{}')",
                    format_task_label(task),
                    current_state_raw
                ));
                continue;
            }
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

            let Some(to_state) = next_to else {
                stalled_ready_tasks.push(format_task_label(task));
                continue;
            };

            if opts.dry_run() {
                run_info!(
                    "{}",
                    format_dry_run_transition(&task_id_str, current_state_raw, &to_state)
                );
                continue;
            }

            visited_ready_states.insert(visit_key);
            if record_poll_self_loop_if_needed(
                input,
                loaded.rhei.metadata.as_ref(),
                machine,
                task,
                &current_state,
                &to_state,
            )? {
                run_info!(
                    "Task {} poll self-loop scheduled next attempt from '{}'",
                    task_id_str,
                    current_state_raw
                );
                transitions_made += 1;
                advanced_any = true;
                break;
            }

            let task_ids_before: BTreeSet<String> =
                loaded.rhei.tasks.iter().map(|existing| existing.id.to_string()).collect();
            let task_file = loaded.task_file(&task_id_str, input);
            let metadata_file = if workspace::is_workspace(input) {
                input.join("index.rhei.md")
            } else {
                task_file.clone()
            };
            match execute_transition(
                TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
                callback_paths,
                machine,
                &task_id_str,
                &current_state,
                &to_state,
                opts.no_callbacks(),
            ) {
                Ok(effective_to) => {
                    run_info!(
                        "Task {} transitioned: '{}' \u{2192} '{}'",
                        task_id_str,
                        current_state_raw,
                        effective_to
                    );
                    run_info!("  {}", format_task_label(task));
                    if is_terminal_state(&effective_to, machine) {
                        run_info!("  Result: reached terminal state '{}'.", effective_to);
                    } else {
                        run_info!("  Result: now in '{}'.", effective_to);
                    }
                    let reloaded = load_plan(input)?;
                    let discovered = newly_discovered_tasks(&task_ids_before, &reloaded.rhei.tasks);
                    if !discovered.is_empty() {
                        run_info!(
                            "  Workspace expanded: discovered {} new task(s): {}",
                            discovered.len(),
                            discovered.join(", ")
                        );
                    }
                    transitions_made += 1;
                    advanced_any = true;
                    break;
                }
                Err(err) => {
                    run_warn!("warning: failed to advance Task {}: {}", task_id_str, err);
                    continue;
                }
            }
        }

        if !stalled_ready_tasks.is_empty() && !advanced_any {
            run_info!(
                "No forward transition available for ready task(s): {}",
                stalled_ready_tasks.join(", ")
            );
        }

        sink.emit(RunEvent::PassEnded { pass, progressed: advanced_any });

        if opts.dry_run() || !advanced_any {
            break;
        }
    }

    let (terminal_count, total_tasks) = if opts.dry_run() {
        run_info!("\nDry run complete \u{2014} no changes were made.");
        (0usize, 0usize)
    } else if transitions_made == 0 {
        run_info!("No tasks could be advanced.");
        (0usize, 0usize)
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = terminal_task_count(&loaded.rhei, machine);
        let total_tasks = total_task_count(&loaded.rhei);
        run_info!(
            "\nRun complete: {} transition(s) made, {}/{} tasks in terminal state.",
            transitions_made,
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

    sink.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned: 0,
            programs_spawned: 0,
            terminal_tasks: terminal_count,
            total_tasks,
            accounting: None,
        },
    });
    frontend.write_frozen_dashboard();
    drop(sink);
    drop(frontend);

    // §FS-rhei-run-report.1/.3: durable report (including dry runs) + console
    // summary. Callback mode spawns no agents/programs; its advances are
    // callback-only. Disarm the guard so its fallback only fires on early error.
    emit_run_report(
        input,
        machine,
        &summary_sink,
        &runtime_dir,
        RunStats {
            agents_spawned: 0,
            programs_spawned: 0,
            callback_only: transitions_made,
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
            mode: "callback",
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

/// Emit the "agent exited 0 but ..." warning(s) after a 0-exit run that did
/// not advance the task. When required outputs are missing, the warning
/// includes the missing names.
// §FS-rhei-agents.3.2.1: Missing-output warning contents.
#[allow(clippy::too_many_arguments)]
fn emit_exit_zero_warnings(
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    task_id_str: &str,
    state_name: &str,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let missing =
        collect_missing_required_outputs(workspace_root, machine, metadata, task, state_name);
    if missing.is_empty() {
        sink.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: format!(
                "  warning: agent exited 0 but task {} did not advance from '{}'",
                task_id_str, state_name
            ),
        });
    } else {
        sink.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: format!(
                "  warning: agent exited 0 but required outputs are missing for task {} in state '{}': {}",
                task_id_str,
                state_name,
                missing.join(", ")
            ),
        });
    }
}

fn emit_exit_zero_missing_required_outputs_warning(
    task_id_str: &str,
    state_name: &str,
    missing: &[String],
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Warn,
        text: format!(
            "  warning: agent exited 0 but required outputs are missing for task {} in state '{}': {}",
            task_id_str,
            state_name,
            missing.join(", ")
        ),
    });
}

/// Walk all resolved invocations for this state and collect the union of
/// required output artifact names that do not exist on disk.
fn collect_missing_required_outputs(
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
) -> Vec<String> {
    let Some(state_def) = machine.states.get(state_name) else {
        return Vec::new();
    };
    if state_def.outputs.is_empty() {
        return Vec::new();
    }
    // This warning path cannot return a settings error after the run has
    // already spawned. Validation loads settings earlier and reports real
    // runtime configuration failures before execution starts.
    let settings = load_merged_settings(workspace_root)
        .unwrap_or_else(|_| RheiSettings { agents: built_in_agents(), ..Default::default() });
    let invocations =
        resolve_agent_invocations(machine, state_name, &settings, &default_run_options())
            .unwrap_or_default();
    let mut missing: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let visit_count =
        Some(render_visit_count(metadata, &task.id, state_name, task.state.as_str(), machine));
    let contexts: Vec<TransitionInvocationContext<'_>> = if invocations.is_empty() {
        transition_contexts_for_state(state_def, &invocations).into_iter().collect()
    } else {
        invocations
            .iter()
            .map(|resolved| {
                (
                    resolved.target.as_ref(),
                    resolved.model.as_deref(),
                    resolved.model_provider.as_deref(),
                    resolved.model_name.as_deref(),
                    Some(resolved.agent.id()),
                    resolved.mode.as_deref(),
                )
            })
            .collect()
    };
    for (target, model, model_provider, model_name, agent, agent_mode) in contexts {
        for artifact in &state_def.outputs {
            let (_, path) = resolve_artifact_path(
                workspace_root,
                artifact,
                &task.id.to_string(),
                state_name,
                visit_count,
                target,
                model,
                model_provider,
                model_name,
                agent,
                agent_mode,
            );
            if !path.exists() && seen.insert(artifact.name.clone()) {
                missing.push(artifact.name.clone());
            }
        }
    }
    missing
}

fn collect_missing_required_outputs_for_resolved_invocation(
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
    resolved: &ResolvedAgent,
) -> Vec<String> {
    let Some(state_def) = machine.states.get(state_name) else {
        return Vec::new();
    };
    if state_def.outputs.is_empty() {
        return Vec::new();
    }

    let mut missing = Vec::new();
    let visit_count =
        Some(render_visit_count(metadata, &task.id, state_name, task.state.as_str(), machine));
    for artifact in &state_def.outputs {
        let (_, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            &task.id.to_string(),
            state_name,
            visit_count,
            resolved.target.as_ref(),
            resolved.model.as_deref(),
            resolved.model_provider.as_deref(),
            resolved.model_name.as_deref(),
            Some(resolved.agent.id()),
            resolved.mode.as_deref(),
        );
        if !path.exists() {
            missing.push(artifact.name.clone());
        }
    }
    missing
}
