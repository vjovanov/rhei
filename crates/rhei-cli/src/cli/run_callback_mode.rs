
/// Callback-only execution mode (legacy behavior, used with --no-agent).
fn run_callback_mode(
    input: &Path,
    machines: &ExecutionMachines,
    opts: &RunOptions,
    max_parallel: usize,
    identity: &RunIdentity,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent, RunSummary};

    // No `RunSubprocessGuard`: this mode spawns nothing supervised, so a guard
    // would own nothing. Anything supervised added here must install one.
    // §FS-rhei-run.3.2
    let workspace_root = run_execution_root(input);
    let runtime_dir = workspace_root.join("runtime");
    // §FS-rhei-run-report.3.1: run duration shown in the end-of-run summary.
    // §FS-rhei-run.2.7: one identity per run, so the report and the descriptor
    // name the same run.
    let run_started = identity.started;
    let run_started_wall = identity.started_wall;
    let run_id = identity.id.clone();
    let command = current_command_line();
    let initial = load_plan(input)?;
    let initial_total_tasks = total_task_count(&initial.rhei);
    let initial_states = collect_initial_states(&initial.rhei, &machines.set);
    // §FS-rhei-run-report.1: declared before the frontend so it drops after the
    // terminal is restored; disarmed on the happy path (see end of run).
    let mut report_guard = RunReportGuard {
        input,
        machines: &machines.set,
        runtime_dir: runtime_dir.clone(),
        run_started,
        run_started_wall,
        run_id: run_id.clone(),
        workspace_root: workspace_root.clone(),
        command: command.clone(),
        parallel: max_parallel,
        mode: "callback",
        initial_states: initial_states.clone(),
        dry_run: opts.dry_run(),
        summary: None,
        armed: true,
    };
    let frontend_parallel = max_parallel.max(1).min(u16::MAX as usize) as u16;
    // Callback-only mode installs no subprocess guard, so nothing raises this
    // and the surface leaves on a signal alone. §FS-rhei-run-tui.1.5.7
    let frontend = start_run_frontend(
        &workspace_root,
        input,
        machines,
        opts,
        frontend_parallel,
        initial_total_tasks,
        &RunShutdown::default(),
        identity,
    );
    let sink = frontend.sink.clone();
    // Route leaf-helper diagnostics through the frontend for the run's duration
    // instead of letting them write straight to the terminal and corrupt the
    // TUI. §FS-rhei-run-tui.1.8
    let diag_guard = RunDiagGuard::install(sink.clone());
    // Held past the frontend drop so the end-of-run summary can read activity
    // after the TUI restores the terminal. §FS-rhei-run-report.3
    let summary_sink = frontend.summary.clone();
    report_guard.summary = Some(summary_sink.clone());
    let dashboard_enabled = frontend.dashboard.is_some();
    sink.emit(RunEvent::RunStarted {
        run_id: run_id.clone(),
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

    let initial_terminal_count = terminal_task_count(&initial.rhei, &machines.set);
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
    // One-time notice so the gate-wait below does not spam the journal each tick.
    let mut awaiting_gate_announced = false;
    // Manual-only tasks reported by a dry run; the command still exits
    // non-zero once the scan is complete. §FS-rhei-run.4
    let mut manual_only_dry_run: Vec<String> = Vec::new();
    // Set when the pass loop found nothing to schedule and at least one
    // remaining ticket needs a human. A dry run must end the same way the real
    // run does, or it is not a prediction. §FS-rhei-run.4
    let mut halted_needs_human = false;
    // §FS-rhei-panta.6.1: `--rhei` narrows candidates, not prior resolution.
    let rhei_scope = rhei_scope_set(opts.rhei_scope());
    if rhei_scope.is_some() {
        // Repeat the pre-launch scope report inside the journal so a TUI run
        // sees it too. §FS-rhei-panta.6
        run_info!("Scope: narrowed to {}", scope_label(&rhei_scope));
    }

    loop {
        // Callback-only advancement spawns no supervised subprocess, but the
        // run still stops when the operator asks it to. §FS-rhei-run.3.2
        if interrupt_requested() {
            // Through the journal, not stderr: when no subprocess is in flight
            // the operator may still be looking at a live TUI.
            // §FS-rhei-run-tui.1.8
            if let Some(notice) = take_interruption_announcement() {
                run_warn!("{notice}");
            }
            break;
        }
        let loaded = load_plan(input)?;
        // §AR-rhei-panta.5: every look at this pass's ready set — the scan, the
        // held-ticket pass, and the halt report that explains what it refused —
        // resolves artifacts under the roots the loaded plan gives its tickets.
        let roots =
            ReadySetRoots { workspace_root: &workspace_root, task_roots: &loaded.task_roots };
        let ready = narrow_to_rhei_scope(
            find_runnable_tasks(&loaded.rhei, &machines.set, &roots, &HashSet::new()),
            &rhei_scope,
        );
        if ready.is_empty() {
            if !opts.dry_run() {
                // Callback-only interactive TUI runs use the same human-gate
                // surface as agent mode; keep it alive only when gates are the
                // remaining blocker. §FS-rhei-run-tui.1.5.5
                if opts.waits_for_human_gates(frontend.is_tui)
                    && should_wait_for_human_gate(&loaded.rhei, &machines.set, &rhei_scope)
                {
                    if !awaiting_gate_announced {
                        run_info!("{}", awaiting_gate_notice(frontend.is_tui));
                        awaiting_gate_announced = true;
                    }
                    interruptible_sleep(Duration::from_millis(500));
                    continue;
                }
                if let Some(deadline) =
                    earliest_pending_poll_deadline(&loaded.rhei, &machines.set, &rhei_scope)
                {
                    let sleep_secs = deadline.saturating_sub(current_unix_secs()).max(1);
                    run_info!(
                        "No ready tasks; sleeping {}s until the next poll attempt.",
                        sleep_secs
                    );
                    interruptible_sleep(Duration::from_secs(sleep_secs));
                    continue;
                }
            }
            // Nothing schedulable and nothing advanced: without this the loop
            // exits having explained nothing, and the dry run reported success
            // on a project the real run halts on. §FS-rhei-run.4
            if pass == 0 {
                // The same roots the ready-set scan just used, so a halt line
                // names the file it looked for. §AR-rhei-panta.5
                let (lines, needs_human) =
                    halted_task_report(&loaded.rhei, &machines.set, &rhei_scope, input, &roots);
                if !lines.is_empty() {
                    run_info!("\nNothing to schedule. Why each remaining ticket is not moving:");
                    for line in &lines {
                        run_info!("  {line}");
                    }
                }
                halted_needs_human = needs_human;
            }
            break;
        }
        // Made progress this pass; re-arm the gate-wait notice for any later gate.
        awaiting_gate_announced = false;

        pass += 1;
        let terminal_count = terminal_task_count(&loaded.rhei, &machines.set);
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
        // A ticket someone already claimed is ready but unschedulable; saying
        // nothing made it look like it was not ready at all. §FS-rhei-run.3
        let held =
            narrow_to_rhei_scope(find_held_tasks(&loaded.rhei, &machines.set, &roots), &rhei_scope);
        if !held.is_empty() {
            run_info!("Held by an assignee, so not scheduled: {}", format_held_tasks(&held));
        }
        // §FS-rhei-supervision.3.4: a dry run is what an author reads to
        // understand a machine, and the barrier is otherwise invisible in it.
        if opts.dry_run() {
            for line in format_supervisor_holds(&loaded.rhei, &machines.set, &rhei_scope) {
                run_info!("{line}");
            }
        }

        let mut advanced_any = false;
        let mut stalled_ready_tasks = Vec::new();

        for task in &ready {
            let task_id_str = task.id.to_string();
            // The ticket's own machine and callback base govern its advance.
            // §DA-per-rhei-state-machines
            let machine = machines.for_task(&task.id);
            let callback_paths = machines.callbacks_for_str(&task_id_str);
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
                // A dry run reports and keeps scanning; only a real run must
                // stop before touching the task. §FS-rhei-run.4
                if opts.dry_run() {
                    let line = format_dry_run_manual_only(&task_id_str, &current_state, &to_state);
                    run_info!("{}", line);
                    manual_only_dry_run.push(line);
                    continue;
                }
                return Err(miette!(
                    help = nothing_claimable_help(),
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
                    format_dry_run_transition(&task_id_str, current_state_raw, &to_state, machine)
                );
                continue;
            }

            visited_ready_states.insert(visit_key);
            if record_poll_self_loop_if_needed(
                &loaded,
                input,
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
            let route = loaded.task_route(&task_id_str, input);
            // Callback-only advancement: no subprocess ran here, so a terminal
            // edge records the engine's own account unless a callback already
            // wrote a result, which wins. §FS-rhei-run.3
            match execute_callback_only_transition(
                TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: &task_id_str },
                callback_paths,
                machine,
                &route.local_id,
                &current_state,
                &to_state,
                opts.no_callbacks(),
                &runtime_dir,
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

    // Read once, here, and used for every statement the run makes about
    // itself: a signal arriving later — while the TUI is parked on its
    // finished screen — did not cut this loop short. §FS-rhei-run.3.2
    let interrupted_run = interrupted_by_signal();

    let (terminal_count, total_tasks) = if opts.dry_run() {
        run_info!("\nDry run complete \u{2014} no changes were made.");
        if !manual_only_dry_run.is_empty() {
            return Err(manual_only_dry_run_error(&manual_only_dry_run));
        }
        // The real run halts here; so must the prediction of it.
        // §FS-rhei-run.4
        if halted_needs_human {
            return Err(dry_run_halt_error());
        }
        (0usize, 0usize)
    } else if transitions_made == 0 {
        let loaded = load_plan(input)?;
        run_info!("{}", no_advancement_summary(&loaded.rhei, &machines.set, &rhei_scope));
        (0usize, 0usize)
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = terminal_task_count(&loaded.rhei, &machines.set);
        let total_tasks = total_task_count(&loaded.rhei);
        // §FS-rhei-run.3.2: the run stopped; it did not complete.
        if interrupted_run {
            run_info!(
                "\nRun interrupted after {} transition(s) made; {}/{} tasks in terminal state.",
                transitions_made,
                terminal_count,
                total_tasks
            );
        } else {
            run_info!(
                "\nRun complete: {} transition(s) made, {}/{} tasks in terminal state.",
                transitions_made,
                terminal_count,
                total_tasks
            );
        }
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
            workspace_accounting: None,
        },
    });
    frontend.write_frozen_dashboard();
    drop(diag_guard);
    drop(sink);
    drop(frontend);

    // §FS-rhei-run-report.1/.3: durable report (skipped under --dry-run, §3.5) +
    // console summary. Callback mode spawns no agents/programs; its advances are
    // callback-only. Disarm the guard so its fallback only fires on early error.
    emit_run_report(
        input,
        &machines.set,
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
            interrupted: interrupted_run,
        },
    );
    report_guard.disarm();

    // An interrupted run is not a halt. The same reading the report used, so
    // the halt decision and the run's own account of itself cannot disagree.
    // §FS-rhei-run.3.2
    if interrupted_run {
        return Ok(());
    }

    if !opts.dry_run() {
        let loaded = load_plan(input)?;
        // §FS-rhei-panta.6.1: a narrowed run halts on in-scope work only —
        // out-of-scope tickets left non-terminal are not a failure.
        if scoped_unfinished_task_exists(&loaded.rhei, &machines.set, &rhei_scope)
            && !remaining_work_is_only_gating_or_poll_blocked(&loaded.rhei, &machines.set, &rhei_scope)
        {
            return Err(miette!(
                help = nothing_claimable_help(),
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
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    task_id_str: &str,
    state_name: &str,
    selected_to: Option<&str>,
    // Carried from the spawn that just finished: only it knows whether the
    // visit has an attempt left. §FS-rhei-agents.3.2.1
    outlook: RetryOutlook,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let missing = collect_missing_required_outputs(
        workspace_root,
        artifact_root,
        machine,
        metadata,
        task,
        state_name,
        selected_to,
    );
    if missing.is_empty() {
        sink.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: format!(
                "  warning: agent exited 0 but task {} did not advance from '{}'",
                task_id_str, state_name
            ),
        });
    } else {
        emit_exit_zero_missing_required_outputs_warning(
            "agent",
            task_id_str,
            state_name,
            &missing,
            outlook,
            sink,
        );
    }
}

/// The stall an operator reads, plus the same facts as data.
///
/// The run report classifies a halted ticket from the structured event; without
/// it the only record of *which* artifact was missing was this line's prose,
/// and the report fell back to "stalled in non-terminal state <s> — inspect
/// logs", which names nothing the operator can act on.
///
/// `worker` is `agent` or `program`. A program is a worker like any other and
/// stalls the same way, so it must reach the report the same way; only the noun
/// in the sentence differs.
// §FS-rhei-agents.3.2.1 §FS-rhei-run-report.3.1
fn emit_exit_zero_missing_required_outputs_warning(
    worker: &str,
    task_id_str: &str,
    state_name: &str,
    missing: &[String],
    // What the run will actually do next, which is not decided by the missing
    // artifacts alone. §FS-rhei-agents.3.2.1
    outlook: RetryOutlook,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Warn,
        text: format!(
            "  warning: {} exited 0 but required outputs are missing for task {} in state '{}': {}",
            worker,
            task_id_str,
            state_name,
            missing.join(", ")
        ),
    });
    // The warning says *what* is missing; this says what the run is doing about
    // it — and after the last budgeted attempt what it does is nothing, so this
    // is conditioned on the budget. §FS-rhei-agents.3.2.1
    sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Warn,
        text: outlook.halt_line(task_id_str, state_name, missing),
    });
    sink.emit(rhei_tui::RunEvent::TaskOutputsMissing {
        task: task_id_str.to_string(),
        state: state_name.to_string(),
        entries: missing.to_vec(),
    });
}

/// Render one missing required output as `name (path)`, flagging a path that
/// still carries a `{...}` template — that means the path referenced a variable
/// outside the namespace, which artifact resolution leaves verbatim by design.
// §FS-rhei-agents.3.2.1: Missing-output warning names the resolved path.
fn format_missing_required_output(name: &str, relative: &str) -> String {
    if relative.contains('{') {
        format!("{name} ({relative}, unresolved template)")
    } else {
        format!("{name} ({relative})")
    }
}

/// The transition `rhei run` would select for `task` from its current state.
///
/// Used only to decide whether the completion condition includes the terminal
/// result. A selection error is not this check's business — the auto-advance
/// path reports it — so it reads as "no edge selected".
// §FS-rhei-run.3
fn selected_forward_transition(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
) -> Option<String> {
    find_next_transition(task, rhei, machine).ok().flatten()
}

/// The ticket's terminal result, rendered as the missing required output it is.
///
/// A `final: true` state requires a non-empty `runtime/results/<task-id>.md` on
/// the edge into it, and under `orchestrator` authority the subprocess is the
/// worker that knows why the ticket is finishing — it was shown the path in its
/// prompt. A zero exit that selects a terminal edge
/// with nothing written therefore fails the completion condition and is
/// reported and routed exactly like any other missing required output, under
/// the artifact name `result`.
///
/// `invocation` names the invocation being judged, so a fanned-out state is
/// judged per invocation exactly as its declared `outputs:` are: one worker's
/// fragment never excuses a sibling that wrote nothing, and a fragment from an
/// earlier fanned-out state or an earlier visit never excuses this one.
///
/// The path is rendered **absolute**. Declared outputs render relative to the
/// workspace root, but in a Panta project the result lives under the owning
/// rhei's root, and a relative path resolved against the wrong root is one an
/// operator cannot paste.
// §FS-rhei-states.3.3 §FS-rhei-agents.3.2 §FS-rhei-agents.3.2.1 §FS-rhei-run.3
fn missing_terminal_result_output(
    result_root: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    selected_to: Option<&str>,
    invocation: ResultInvocation<'_>,
) -> Option<String> {
    if !is_terminal_state(selected_to?, machine) {
        return None;
    }
    let task_id = task.id.to_string();
    // The result lives under the owning rhei's execution root, which is where
    // the transition path will look for it. §FS-rhei-panta.6.2
    let path = invocation_result_file_path(result_root, &task_id, invocation);
    if file_has_content(&path) {
        return None;
    }
    let shown = std::path::absolute(&path).unwrap_or(path);
    Some(format_missing_required_output("result", &shown.display().to_string()))
}

/// Walk all resolved invocations for this state and collect the union of
/// required output artifacts that do not exist on disk, each rendered as
/// `name (resolved/path)` so the warning points at the file that was checked.
///
/// `selected_to` is the transition the exit would take; when it lands on a
/// `final: true` state the ticket's terminal result joins the list — once per
/// fan-out identity, because that is how many result fragments the state was
/// asked for.
///
/// `workspace_root` is only for re-resolving invocation settings; declared
/// `outputs:` and the terminal result resolve against `artifact_root`, the
/// owning rhei's execution root — the two must stay distinct rather than
/// merged into one same-typed root. §FS-rhei-agents.3.2 condition (2)
// §FS-rhei-agents.3.2.1 §FS-rhei-states.3.3: the warning names resolved paths.
#[allow(clippy::too_many_arguments)]
fn collect_missing_required_outputs(
    workspace_root: &Path,
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
    selected_to: Option<&str>,
) -> Vec<String> {
    let Some(state_def) = machine.states.get(state_name) else {
        return missing_terminal_result_output(
            artifact_root,
            machine,
            task,
            selected_to,
            ResultInvocation::whole_task(),
        )
        .into_iter()
        .collect();
    };
    // Walked even with no declared `outputs:`: fragments are per invocation, so
    // the union needs the invocation list. A `program:` state never fans out,
    // however many targets it names. §FS-rhei-programs.2
    let fans_out = state_def.program.is_none()
        && (!state_def.all_targets.is_empty() || !state_def.all_models.is_empty());
    if state_def.outputs.is_empty() && !fans_out {
        return missing_terminal_result_output(
            artifact_root,
            machine,
            task,
            selected_to,
            ResultInvocation::whole_task(),
        )
        .into_iter()
        .collect();
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
    let visit = render_visit_count(metadata, &task.id, state_name, task.state.as_str(), machine);
    let visit_count = Some(visit);
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
    let mut terminal_results: Vec<String> = Vec::new();
    for (target, model, model_provider, model_name, agent, agent_mode) in contexts {
        for artifact in &state_def.outputs {
            let (relative, path) = resolve_artifact_path(
                artifact_root,
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
            if path.exists() {
                continue;
            }
            // Dedup on the resolved path, not the name: a fanned-out state
            // resolves one artifact name to a distinct path per target, and
            // each missing path is worth naming.
            let entry = format_missing_required_output(&artifact.name, &relative);
            if seen.insert(entry.clone()) {
                missing.push(entry);
            }
        }
        let identity = fanout_result_identity(Some(state_def), target, model);
        if let Some(entry) = missing_terminal_result_output(
            artifact_root,
            machine,
            task,
            selected_to,
            ResultInvocation {
                state: state_name,
                visit_count: visit,
                identity: identity.as_deref(),
            },
        ) {
            if !terminal_results.contains(&entry) {
                terminal_results.push(entry);
            }
        }
    }
    missing.extend(terminal_results);
    missing
}

// The invocation is already resolved, so there is no settings re-load here —
// unlike `collect_missing_required_outputs`, one root suffices.
// §FS-rhei-agents.3.2 condition (2)
#[allow(clippy::too_many_arguments)]
fn collect_missing_required_outputs_for_resolved_invocation(
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
    selected_to: Option<&str>,
    resolved: &ResolvedAgent,
) -> Vec<String> {
    let visit = render_visit_count(metadata, &task.id, state_name, task.state.as_str(), machine);
    let visit_count = Some(visit);
    let terminal_result = missing_terminal_result_output(
        artifact_root,
        machine,
        task,
        selected_to,
        ResultInvocation {
            state: state_name,
            visit_count: visit,
            identity: fanout_result_identity(
                machine.states.get(state_name),
                resolved.target.as_ref(),
                resolved.model.as_deref(),
            )
            .as_deref(),
        },
    );
    let Some(state_def) = machine.states.get(state_name) else {
        return terminal_result.into_iter().collect();
    };
    if state_def.outputs.is_empty() {
        return terminal_result.into_iter().collect();
    }

    let mut missing = Vec::new();
    for artifact in &state_def.outputs {
        let (relative, path) = resolve_artifact_path(
            artifact_root,
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
            missing.push(format_missing_required_output(&artifact.name, &relative));
        }
    }
    missing.extend(terminal_result);
    missing
}
