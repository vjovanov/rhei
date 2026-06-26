
/// Flags that control standalone execution behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Standalone Execution")]
struct StandaloneExecutionFlags {
    /// Show what transitions would be made without executing them
    #[arg(long)]
    dry_run: bool,
    /// Skip execution of on_leave/on_enter callbacks
    #[arg(long)]
    no_callbacks: bool,
    /// Continue to the next task when an agent exits non-zero
    #[arg(long)]
    continue_on_error: bool,
    /// Maximum number of agents to run concurrently (0 = unlimited)
    #[arg(long, default_value_t = 1, add = ArgValueCompleter::new(complete_parallel))]
    parallel: usize,
    /// Force TUI mode even when stdout is not detected as a TTY
    #[arg(long, conflicts_with = "no_tui")]
    tui: bool,
    /// Force plain stdout output even when stdout is a TTY
    #[arg(long)]
    no_tui: bool,
    /// Serve a loopback browser dashboard for this run
    #[arg(long, conflicts_with = "no_dashboard")]
    dashboard: bool,
    /// Disable the loopback browser dashboard
    #[arg(long)]
    no_dashboard: bool,
}

/// Flags that control agent-specific behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Agent Execution")]
struct AgentExecutionFlags {
    /// Disable agent spawning; use callback-only advancement
    #[arg(long)]
    no_agent: bool,
    /// Override the agent for this run
    #[arg(long, value_name = "AGENT", add = ArgValueCompleter::new(complete_agent_name))]
    agent: Option<String>,
    /// Override the agent mode (named flag set) for this run
    #[arg(long, value_name = "MODE", add = ArgValueCompleter::new(complete_agent_mode))]
    agent_mode: Option<String>,
    /// Override the model for this run
    #[arg(long, value_name = "MODEL", add = ArgValueCompleter::new(complete_model_name))]
    model: Option<String>,
}

/// Flags that control program-specific behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Program Execution")]
struct ProgramExecutionFlags {
    /// Disable program spawning; use callback-only advancement for program states
    #[arg(long)]
    no_program: bool,
    /// Override the program timeout for this run
    #[arg(long, value_name = "DURATION", add = ArgValueCompleter::new(complete_duration))]
    program_timeout: Option<String>,
}

/// Flags that control snapshot inheritance overrides for `rhei run`.
///
/// §FS-rhei-run.2.3 §FS-rhei-snapshot-operations.2: Snapshot run flags.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Snapshots")]
struct SnapshotExecutionFlags {
    /// Override the concrete source snapshot selected by an authored
    /// `snapshot.inherit:` after that state's constraints are applied.
    #[arg(long, value_name = "REF")]
    from_snapshot: Option<String>,
    /// Explicitly bypass authored source-selection and compatibility
    /// constraints for an ad-hoc debug run. Requires `--from-snapshot`.
    #[arg(long, requires = "from_snapshot")]
    override_inherit: bool,
    /// Select the task for an ambiguous snapshot override.
    #[arg(long = "task", value_name = "TASK_ID", add = ArgValueCompleter::new(complete_task_id))]
    snapshot_task: Option<String>,
    /// Select the fanout target for an ambiguous snapshot override.
    #[arg(long = "target", value_name = "SLUG")]
    snapshot_target: Option<String>,
}

/// Options for the `run` command.
struct RunOptions {
    standalone: StandaloneExecutionFlags,
    agent: AgentExecutionFlags,
    program: ProgramExecutionFlags,
    snapshot: SnapshotExecutionFlags,
}

impl RunOptions {
    fn dry_run(&self) -> bool {
        self.standalone.dry_run
    }

    fn no_callbacks(&self) -> bool {
        self.standalone.no_callbacks
    }

    fn continue_on_error(&self) -> bool {
        self.standalone.continue_on_error
    }

    fn parallel(&self) -> usize {
        self.standalone.parallel
    }

    fn frontend_kind(&self) -> rhei_tui::FrontendKind {
        if self.standalone.tui {
            rhei_tui::FrontendKind::Tui
        } else if self.standalone.no_tui {
            rhei_tui::FrontendKind::Stdout
        } else {
            rhei_tui::FrontendKind::Auto
        }
    }

    fn dashboard_enabled(&self, frontend_is_tui: bool) -> bool {
        if self.standalone.dashboard {
            true
        } else if self.standalone.no_dashboard {
            false
        } else {
            frontend_is_tui
        }
    }

    fn no_agent(&self) -> bool {
        self.agent.no_agent
    }

    fn agent_override(&self) -> Option<&str> {
        self.agent.agent.as_deref()
    }

    fn agent_mode_override(&self) -> Option<&str> {
        self.agent.agent_mode.as_deref()
    }

    fn model_override(&self) -> Option<&str> {
        self.agent.model.as_deref()
    }

    fn no_program(&self) -> bool {
        self.program.no_program
    }

    fn program_timeout_override(&self) -> Option<&str> {
        self.program.program_timeout.as_deref()
    }

    fn snapshot_override_ref(&self) -> Option<&str> {
        self.snapshot.from_snapshot.as_deref()
    }

    fn override_inherit(&self) -> bool {
        self.snapshot.override_inherit
    }

    fn snapshot_task_selector(&self) -> Option<&str> {
        self.snapshot.snapshot_task.as_deref()
    }

    fn snapshot_target_selector(&self) -> Option<&str> {
        self.snapshot.snapshot_target.as_deref()
    }
}

struct ActiveRunFrontend {
    sink: Arc<dyn rhei_tui::EventSink>,
    /// True when an interactive TUI is the active frontend. The run loop uses
    /// this to keep itself alive while a human gate is pending, so the operator
    /// can resolve the gate in the UI and have the run continue (§FS-rhei-run-tui.1.5.5).
    is_tui: bool,
    dashboard: Option<Arc<rhei_tui::DashboardSink>>,
    /// Accumulates per-task driver/duration for the end-of-run console summary.
    /// §FS-rhei-run-report.3
    summary: Arc<SummarySink>,
    /// The intervene registry, present only when the dashboard is live. The run
    /// loop registers each running agent's stdin here so `/intervene` can reach
    /// it. AR §7.
    intervene: Option<Arc<RunInterveneSink>>,
    _frontend: Option<rhei_tui::Frontend>,
}

struct RunGateTransitionSink {
    input: PathBuf,
    machine: rhei_validator::StateMachine,
    callback_paths: CallbackPaths,
    no_callbacks: bool,
}

impl RunGateTransitionSink {
    fn new(
        input: PathBuf,
        machine: rhei_validator::StateMachine,
        callback_paths: CallbackPaths,
        no_callbacks: bool,
    ) -> Self {
        Self { input, machine, callback_paths, no_callbacks }
    }
}

impl rhei_tui::GateTransitionSink for RunGateTransitionSink {
    fn transition_gate(&self, task_id: &str, from: &str, to: &str) -> Result<String, String> {
        transition_dashboard_gate(
            &self.input,
            &self.machine,
            &self.callback_paths,
            task_id,
            from,
            to,
            self.no_callbacks,
        )
        .map_err(|err| err.to_string())
    }
}

impl ActiveRunFrontend {
    fn announce_dashboard(&self) {
        if let Some(dashboard) = &self.dashboard {
            self.sink.emit(rhei_tui::RunEvent::RunLink {
                label: "Dashboard".to_string(),
                url: dashboard.url().to_string(),
            });
        }
    }

    fn write_frozen_dashboard(&self) {
        let Some(dashboard) = &self.dashboard else {
            return;
        };
        match dashboard.write_frozen_dashboard() {
            Ok(path) => self.sink.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Info,
                text: format!("Final dashboard: {}", path.display()),
            }),
            Err(err) => self.sink.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Warn,
                text: format!("warning: could not write final dashboard: {err}"),
            }),
        }
    }
}

fn start_run_frontend(
    workspace_root: &Path,
    plan_input: &Path,
    callback_paths: &CallbackPaths,
    opts: &RunOptions,
    parallel: u16,
    total_tasks: usize,
    machine: &rhei_validator::StateMachine,
) -> ActiveRunFrontend {
    if opts.dry_run() {
        return ActiveRunFrontend {
            sink: Arc::new(rhei_tui::StdoutSink::new()),
            is_tui: false,
            dashboard: None,
            summary: Arc::new(SummarySink::new()),
            intervene: None,
            _frontend: None,
        };
    }

    // The loader re-reads the plan and builds the full `VizModel` via `rhei-viz`,
    // so the TUI render thread and dashboard share one run model and the same
    // intervene/gate boundaries; neither parses plans itself. §FS-rhei-run-tui.1.5
    let plan_path = plan_input.to_path_buf();
    let loader_machine = machine.clone();
    let loader: rhei_tui::PlanLoader =
        Arc::new(move || load_plan_for_dashboard(&plan_path, &loader_machine));
    // AR §7: the intervene registry the run loop registers agents into.
    let registry = Arc::new(RunInterveneSink::new(workspace_root.join("runtime")));
    let gate = Arc::new(RunGateTransitionSink::new(
        plan_input.to_path_buf(),
        machine.clone(),
        callback_paths.clone(),
        opts.no_callbacks(),
    ));

    let tui_context = rhei_tui::TuiContext {
        workspace: workspace_root.to_path_buf(),
        plan_loader: Some(loader.clone()),
        intervene: Some(registry.clone() as Arc<dyn rhei_tui::InterveneSink>),
        gate: Some(gate.clone() as Arc<dyn rhei_tui::GateTransitionSink>),
    };
    let frontend = rhei_tui::select_frontend(
        workspace_root,
        opts.frontend_kind(),
        parallel,
        total_tasks,
        tui_context,
    );

    let dashboard = if opts.dashboard_enabled(frontend.is_tui) {
        match rhei_tui::DashboardSink::start_with_plan_intervene_and_gate(
            workspace_root.to_path_buf(),
            parallel,
            total_tasks,
            Some(loader.clone()),
            Some(registry.clone() as Arc<dyn rhei_tui::InterveneSink>),
            Some(gate.clone() as Arc<dyn rhei_tui::GateTransitionSink>),
        ) {
            Ok(sink) => Some(Arc::new(sink)),
            Err(err) => {
                frontend.sink.emit(rhei_tui::RunEvent::Message {
                    level: rhei_tui::MessageLevel::Warn,
                    text: format!("warning: could not start dashboard: {err}"),
                });
                None
            }
        }
    } else {
        None
    };

    // The run loop registers running agents' stdin into the registry so both the
    // TUI composer and the dashboard `/intervene` can reach them. Wire it
    // whenever a live surface is present.
    let intervene: Option<Arc<RunInterveneSink>> =
        (frontend.is_tui || dashboard.is_some()).then(|| registry.clone());

    // The summary sink is always teed in so the end-of-run console summary can
    // render per-task driver/duration regardless of dashboard state.
    // §FS-rhei-run-report.3
    let summary = Arc::new(SummarySink::new());
    let mut inner: Vec<Arc<dyn rhei_tui::EventSink>> =
        vec![frontend.sink.clone(), summary.clone()];
    if let Some(dashboard) = &dashboard {
        inner.push(dashboard.clone());
    }
    let sink: Arc<dyn rhei_tui::EventSink> = Arc::new(rhei_tui::Tee::new(inner));

    let is_tui = frontend.is_tui;
    ActiveRunFrontend { sink, is_tui, dashboard, summary, intervene, _frontend: Some(frontend) }
}

fn transition_dashboard_gate(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<String> {
    let loaded = load_plan(input)?;
    let task = find_task_by_id_str(&loaded.rhei.tasks, task_id_str)
        .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
    let current_state = normalized_state_name(task.state.as_str(), machine);
    if current_state != from {
        return Err(miette!(
            "conflict: Task {} is in state '{}', expected '{}'",
            task_id_str,
            task.state,
            from
        ));
    }
    if !machine.states.get(&current_state).map(|def| def.gating).unwrap_or(false) {
        return Err(miette!(
            "Task {} is in state '{}', which is not a gating state",
            task_id_str,
            current_state
        ));
    }
    let explicit_transition =
        machine.transitions().iter().any(|rule| rule.from.0 == from && rule.to.0 == to);
    if !explicit_transition {
        return Err(miette!(
            "transition from '{}' to '{}' is not an explicit human-gate transition",
            from,
            to
        ));
    }

    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };
    let effective_to = execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
    )?;
    let root = result_workspace_root(input, &task_file);
    record_transition_result(&root, &task_file, machine, task_id_str, from, &effective_to, None)?;
    Ok(effective_to)
}

/// Re-read the plan from disk and build the dashboard's [`VizModel`] via
/// `rhei-viz` (flatten the resolved machine, derive plan state, classify).
/// Called on every `/snapshot` request, so failures must be non-fatal — return
/// `None` and let the dashboard fall back to the last good model. AR §5.2.
fn load_plan_for_dashboard(
    plan_path: &Path,
    machine: &rhei_validator::StateMachine,
) -> Option<rhei_viz_model::VizModel> {
    let loaded = load_plan(plan_path).ok()?;
    Some(rhei_viz::build(&loaded.rhei, machine))
}

impl
    From<(
        StandaloneExecutionFlags,
        AgentExecutionFlags,
        ProgramExecutionFlags,
        SnapshotExecutionFlags,
    )> for RunOptions
{
    fn from(
        (standalone, agent, program, snapshot): (
            StandaloneExecutionFlags,
            AgentExecutionFlags,
            ProgramExecutionFlags,
            SnapshotExecutionFlags,
        ),
    ) -> Self {
        Self { standalone, agent, program, snapshot }
    }
}
