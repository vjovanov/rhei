/// Panta project recipe commands. §FS-rhei-panta.7
#[derive(Subcommand, Debug)]
enum PantaCommand {
    /// Append a rhei entry to the project recipe
    Add {
        /// Stable, unique rhei id for the new recipe entry
        #[arg(value_name = "ID")]
        id: String,
        /// Template to instantiate (resolved like `rhei instantiate`)
        #[arg(long, value_name = "NAME", add = ArgValueCompleter::new(templates::complete_template_reference))]
        template: String,
        /// Set a template input value (repeatable)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set_values: Vec<String>,
        /// Declare a dependency on an existing recipe entry (repeatable)
        #[arg(long = "depends-on", value_name = "ID")]
        depends_on: Vec<String>,
        /// Panta project directory (defaults to the current project)
        #[arg(long, value_name = "DIR", add = ArgValueCompleter::new(complete_any_path))]
        project: Option<PathBuf>,
    },
    /// Instantiate and run the recipe in dependency order
    Run {
        /// Restrict the run to the named rhei(s) and their dependencies (repeatable)
        #[arg(long = "rhei", value_name = "ID")]
        only: Vec<String>,
        /// Instantiate and validate each rhei without executing agents
        #[arg(long)]
        dry_run: bool,
        /// Panta project directory (defaults to the current project)
        #[arg(long, value_name = "DIR", add = ArgValueCompleter::new(complete_any_path))]
        project: Option<PathBuf>,
    },
}

/// Snapshot cache maintenance commands.
#[derive(Subcommand, Debug)]
enum SnapshotCommand {
    /// List cached snapshot generations
    List {
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Filter by task id
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Filter by snapshot name; use _state for auto-emitted snapshots
        #[arg(long, value_name = "SNAPSHOT")]
        name: Option<String>,
        /// Filter by emitting state
        #[arg(long, value_name = "STATE", add = ArgValueCompleter::new(complete_state_name))]
        state: Option<String>,
        /// Filter by emission origin
        #[arg(long, value_enum, default_value = "orchestrator")]
        produced_by: SnapshotProducedByFilter,
        /// Show only snapshots that no longer resolve in the current plan/state machine
        #[arg(long)]
        orphaned: bool,
        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: SnapshotListFormat,
    },
    /// Show one snapshot manifest and transcript preview
    Show {
        /// Snapshot reference
        #[arg(value_name = "REF")]
        reference: String,
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
    },
    /// Delete cached snapshot generations by policy
    Gc {
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Filter by task id
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Filter by snapshot name
        #[arg(long, value_name = "SNAPSHOT")]
        name: Option<String>,
        /// Delete only generations older than this duration (for example 7d or 4h)
        #[arg(long, value_name = "DURATION")]
        older_than: Option<String>,
        /// Keep the newest N generations per snapshot identity
        #[arg(long, value_name = "N")]
        keep_generations: Option<usize>,
        /// Include operator-produced generations in retention and deletion decisions
        #[arg(long)]
        include_operator: bool,
        /// Delete only snapshots that no longer resolve in the current plan/state machine
        #[arg(long)]
        orphaned: bool,
        /// Print what would be deleted without removing files
        #[arg(long)]
        dry_run: bool,
        /// Bypass the live-run interlock
        #[arg(long)]
        force: bool,
    },
    /// Continue interactively from a cached snapshot
    Continue {
        /// Snapshot reference
        #[arg(value_name = "REF")]
        reference: String,
        /// Path to a plan file or workspace root; defaults to the current directory
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Select a target slug when the reference is ambiguous
        #[arg(long, value_name = "SLUG")]
        target: Option<String>,
        /// Continue from a specific generation
        #[arg(long, value_name = "N")]
        generation: Option<u64>,
        /// Do not capture the resulting operator transcript
        #[arg(long)]
        no_capture: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SnapshotProducedByFilter {
    Orchestrator,
    Operator,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SnapshotListFormat {
    Text,
    Json,
}

/// Output formats supported by the [`Render`](Commands::Render) subcommand.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum RenderFormat {
    Json,
    Github,
    Progress,
}

/// Supported AI coding agents for skill installation.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Agent {
    ClaudeCode,
    Cursor,
    Windsurf,
    Copilot,
    Kilocode,
    Pi,
    Codex,
    Antigravity,
    All,
}

/// Shells supported by the completion generator.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
    Elvish,
}

impl CompletionShell {
    fn as_str(self) -> &'static str {
        match self {
            CompletionShell::Bash => "bash",
            CompletionShell::Zsh => "zsh",
            CompletionShell::Fish => "fish",
            CompletionShell::PowerShell => "powershell",
            CompletionShell::Elvish => "elvish",
        }
    }
}

/// Program entry point.
///
/// Delegates to fallible command logic so tests can exercise it directly.
fn main() {
    CompleteEnv::with_factory(cli_command).bin("rhei").complete();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            let mut cmd = cli_command();
            if let Err(io_err) = cmd.print_help() {
                eprintln!("failed to write CLI help: {io_err}");
                std::process::exit(1);
            }
            println!();
            return;
        }
        Err(err) => err.exit(),
    };

    let json_mode = command_wants_json(&cli.command);

    if let Err(err) = dispatch(cli) {
        if json_mode {
            emit_json_error(&err);
        } else {
            eprintln!("{err:?}");
        }
        std::process::exit(1);
    }
}

/// Returns true when the invoked command's output format is JSON. In that
/// case, errors are rendered as a single-line JSON object on stderr instead
/// of the default miette text, so machine consumers don't have to parse two
/// shapes.
fn command_wants_json(command: &Commands) -> bool {
    match command {
        Commands::Next { json, .. } => *json,
        Commands::States { json } => *json,
        Commands::List { json, .. } => *json,
        Commands::Snapshot { command: SnapshotCommand::List { format, .. } } => {
            matches!(format, SnapshotListFormat::Json)
        }
        Commands::Templates { json, .. } => *json,
        Commands::Cost { json, .. } => *json,
        Commands::Render { format, .. } => matches!(format, RenderFormat::Json),
        _ => false,
    }
}

fn emit_json_error(err: &miette::Report) {
    let payload = serde_json::json!({
        "error": {
            "message": err.to_string(),
        }
    });
    let serialized = serde_json::to_string(&payload)
        .unwrap_or_else(|_| format!("{{\"error\":{{\"message\":{:?}}}}}", err.to_string()));
    eprintln!("{serialized}");
}

/// Dispatch the parsed CLI command.
fn dispatch(cli: Cli) -> MietteResult<()> {
    match cli.command {
        Commands::Validate { watch, input } => {
            validate_command(&input, cli.state_machine.as_deref(), watch)
        }
        Commands::Render { input, format, pretty, no_color, no_metadata, no_content } => {
            render_command(&input, format, pretty, no_color, no_metadata, no_content)
        }
        Commands::States { json } => states_command(cli.state_machine.as_deref(), json),
        Commands::List {
            input,
            state,
            assignee,
            no_assignee,
            kind,
            has_prior,
            parent,
            root,
            contains,
            terminal,
            non_terminal,
            ready,
            blocked,
            limit,
            json,
        } => list_command(
            &input,
            cli.state_machine.as_deref(),
            ListFilters {
                states: state,
                assignee,
                no_assignee,
                kind,
                has_prior,
                parent,
                root,
                contains,
                terminal,
                non_terminal,
                ready,
                blocked,
                limit,
            },
            json,
        ),
        Commands::Transition { input, task, from, to, no_callbacks } => transition_command(
            &input,
            cli.state_machine.as_deref(),
            &task,
            &from,
            &to,
            no_callbacks,
        ),
        Commands::Run { input, standalone, agent, program, snapshot } => run_command(
            &input,
            cli.state_machine.as_deref(),
            (standalone, agent, program, snapshot).into(),
        ),
        Commands::Cost { input, task, json, by } => cost_command(&input, task.as_deref(), json, by),
        Commands::Intervene { plan, task, slot, message } => {
            intervene_command(&plan, &task, slot, &message)
        }
        Commands::Viz { input, output, open } => {
            viz_command(&input, cli.state_machine.as_deref(), output.as_deref(), open)
        }
        Commands::Snapshot { command } => snapshot_command(command, cli.state_machine.as_deref()),
        Commands::Templates { json, source } => templates::templates_command(json, &source),
        Commands::Instantiate {
            template,
            set_values,
            set_files,
            values,
            output,
            execute,
            dry_run,
            keep_on_error,
            list_inputs,
            input_args,
        } => templates::instantiate_command(
            template.as_deref(),
            &input_args,
            &instantiate_execute_args_from_env(),
            &set_values,
            &set_files,
            &values,
            output.as_deref(),
            execute,
            dry_run,
            keep_on_error,
            list_inputs,
        ),
        Commands::Add { source, project, link, force } => {
            templates::add_template_command(&source, project, link, force)
        }
        Commands::Panta { command } => match command {
            PantaCommand::Add { id, template, set_values, depends_on, project } => {
                panta_add_command(&id, &template, &set_values, &depends_on, project.as_deref())
            }
            PantaCommand::Run { only, dry_run, project } => {
                panta_run_command(&only, dry_run, project.as_deref())
            }
        },
        Commands::Next { input, task, json, no_callbacks, peek } => next_command(
            &input,
            cli.state_machine.as_deref(),
            task.as_deref(),
            json,
            no_callbacks,
            peek,
        ),
        Commands::Complete { input, task, result, no_callbacks } => {
            complete_command(&input, cli.state_machine.as_deref(), &task, &result, no_callbacks)
        }
        Commands::Reset { input } => reset_command(&input, cli.state_machine.as_deref()),
        Commands::Version => {
            print_versions();
            Ok(())
        }
        Commands::InstallSkills { agent, local, link, uninstall, dry_run, skills } => {
            install_skills_command(agent, local, link, uninstall, dry_run, &skills)
        }
        Commands::Completions { shell, install, user: _, system, output, dry_run } => {
            completions_command(shell, install, system, output.as_deref(), dry_run)
        }
    }
}
