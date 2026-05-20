
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
    dashboard: Option<Arc<rhei_tui::DashboardSink>>,
    _frontend: Option<rhei_tui::Frontend>,
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
    opts: &RunOptions,
    parallel: u16,
    total_tasks: usize,
) -> ActiveRunFrontend {
    if opts.dry_run() {
        return ActiveRunFrontend {
            sink: Arc::new(rhei_tui::StdoutSink::new()),
            dashboard: None,
            _frontend: None,
        };
    }

    let frontend =
        rhei_tui::select_frontend(workspace_root, opts.frontend_kind(), parallel, total_tasks);
    let dashboard = if opts.dashboard_enabled(frontend.is_tui) {
        let plan_path = plan_input.to_path_buf();
        let loader: rhei_tui::PlanLoader = Arc::new(move || load_plan_for_dashboard(&plan_path));
        match rhei_tui::DashboardSink::start_with_plan(
            workspace_root.to_path_buf(),
            parallel,
            total_tasks,
            Some(loader),
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

    let sink: Arc<dyn rhei_tui::EventSink> = if let Some(dashboard) = &dashboard {
        Arc::new(rhei_tui::Tee::new(vec![frontend.sink.clone(), dashboard.clone()]))
    } else {
        frontend.sink.clone()
    };

    ActiveRunFrontend { sink, dashboard, _frontend: Some(frontend) }
}

/// Re-read the plan from disk and project it into the dashboard's
/// `PlanSnapshot`. Called on every `/snapshot` request, so failures must be
/// non-fatal — return `None` and let the dashboard fall back to the last
/// good snapshot.
fn load_plan_for_dashboard(plan_path: &Path) -> Option<rhei_tui::PlanSnapshot> {
    let loaded = load_plan(plan_path).ok()?;
    let mut tasks = Vec::new();
    fn visit(
        node: &rhei_core::ast::Task,
        depth: u8,
        parent: Option<String>,
        out: &mut Vec<rhei_tui::DashboardTask>,
    ) {
        let id = node.id.to_string();
        out.push(rhei_tui::DashboardTask {
            id: id.clone(),
            title: node.title.clone(),
            kind: node.kind.clone(),
            parent,
            depth,
            state: node.state.clone(),
            assignee: node.assignee.clone(),
            prior: node.prior.iter().map(|p| p.to_string()).collect(),
            // Result link extraction is best-effort; we look for the well-
            // known `> **Result:** [id](path)` line that `rhei complete`
            // emits. Anything else stays None.
            result_link: extract_result_link(&node.content),
        });
        for child in &node.children {
            visit(child, depth + 1, Some(id.clone()), out);
        }
    }
    for root in &loaded.rhei.tasks {
        visit(root, 1, None, &mut tasks);
    }
    Some(rhei_tui::PlanSnapshot { title: loaded.rhei.title, tasks })
}

fn extract_result_link(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("> **Result:**") {
            // Match `[<id>](<path>)` and return `<path>`. Use the *last*
            // parenthesis pair so paths containing `(` or `)` are handled.
            let rparen = rest.rfind(')')?;
            let lparen = rest[..rparen].rfind('(')?;
            return Some(rest[lparen + 1..rparen].to_string());
        }
    }
    None
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
