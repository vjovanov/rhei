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
/// Wrap diagnostics at word boundaries but never *inside* a word.
///
/// miette's defaults offer a break opportunity at every hyphen and every `/`,
/// and split an overlong token outright. All three land mid-path on the
/// filesystem diagnostics this CLI prints constantly, and a path broken across
/// lines cannot be copied, clicked, or grepped. Treating only spaces as break
/// points keeps prose wrapping while a long path overflows the wrap column
/// intact, where the terminal soft-wraps it.
fn install_diagnostic_handler() {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .build(),
        )
    }));
}

/// True when `rhei` was invoked with no arguments at all.
///
/// Distinguishes the orientation case from a subcommand-level usage error;
/// see the call site in [`main`].
fn is_bare_invocation() -> bool {
    std::env::args_os().count() <= 1
}

/// Conventional status for a process that stopped because a pipe consumer
/// closed early: `128 + SIGPIPE`, the same value the shell reports for
/// `yes | head`.
const EXIT_BROKEN_PIPE: i32 = 141;

/// Leave a pipeline quietly when the consumer stops reading, instead of
/// surfacing an internal error.
///
/// Rust ignores `SIGPIPE` before `main`, so a closed stdout comes back as an
/// `EPIPE` write error and `println!` panics on it — `rhei list | head` exited
/// 101 with a stack trace. This intercepts exactly that panic and exits the way
/// a Unix filter killed by the signal does.
///
/// Restoring `SIGPIPE` to `SIG_DFL` process-wide would be the shorter fix and
/// is the wrong one: this CLI writes to pipes it owns — a callback
/// subprocess's stdin, an agent's — and there the write returning `EPIPE` is
/// how a child that exited early gets *reported*. Under `SIG_DFL` those writes
/// killed `rhei` mid-diagnostic instead, so a transition that should have
/// failed with an explanation failed with empty stderr.
// §FS-rhei-usage.2: an early-closed stdout is normal shell usage, not a failure.
fn install_quiet_broken_pipe_exit() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if is_broken_pipe_panic(info) {
            std::process::exit(EXIT_BROKEN_PIPE);
        }
        previous(info);
    }));
}

/// Whether a panic is the standard library's "failed printing to stdout"
/// broken-pipe panic, rather than a real bug.
///
/// Matched on the payload text because that is all the standard library
/// exposes: the panic carries no typed error. Both halves must match, so a
/// panic that merely mentions a broken pipe in some other context still
/// reports normally.
fn is_broken_pipe_panic(info: &std::panic::PanicHookInfo<'_>) -> bool {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    message.is_some_and(|message| {
        message.starts_with("failed printing to std") && message.contains("Broken pipe")
    })
}

pub fn run() {
    install_quiet_broken_pipe_exit();
    install_diagnostic_handler();
    CompleteEnv::with_factory(cli_command).bin(invoked_bin_name()).complete();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // A bare `rhei` is a request for orientation, so answer it with the
        // root help on stdout and a success exit. Every *other* missing
        // subcommand — `rhei snapshot`, say — is a usage error about that
        // subcommand: let clap render its own contextual help to stderr with
        // the conventional exit code, or a script cannot tell the two apart.
        Err(err)
            if is_bare_invocation()
                && matches!(
                    err.kind(),
                    ErrorKind::MissingSubcommand
                        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
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
        Commands::States { json, .. } => *json,
        Commands::List { json, .. } => *json,
        Commands::Snapshot { command: SnapshotCommand::List { format, .. }, .. } => {
            matches!(format, SnapshotListFormat::Json)
        }
        Commands::Templates { json, .. } => *json,
        Commands::Cost { json, .. } => *json,
        Commands::Render { format, .. } => matches!(format, RenderFormat::Json),
        _ => false,
    }
}

fn emit_json_error(err: &miette::Report) {
    // §FS-rhei-errors.5: machine consumers get the same next action as humans.
    let mut error = serde_json::json!({ "message": err.to_string() });
    if let Some(help) = err.help() {
        error["help"] = serde_json::Value::String(help.to_string());
    }
    let payload = serde_json::json!({ "error": error });
    let serialized = serde_json::to_string(&payload)
        .unwrap_or_else(|_| format!("{{\"error\":{{\"message\":{:?}}}}}", err.to_string()));
    eprintln!("{serialized}");
}

/// Dispatch the parsed CLI command.
fn dispatch(cli: Cli) -> MietteResult<()> {
    // `--state-machine` is accepted both before the subcommand and on the
    // subcommands that read one; the subcommand copy wins when both appear.
    let before_subcommand = cli.state_machine;
    match cli.command {
        Commands::Init { dir, here, title, no_agents, force } => {
            init_command(dir.as_deref(), title.as_deref(), no_agents, force, here)
        }
        Commands::Validate { watch, input, state_machine } => {
            // §FS-rhei-validate.1.1: validation never narrows — a member rhei
            // validates the project it cannot resolve without.
            let target = resolve_plan_target(input)?;
            report_validation_widened(&target);
            validate_command(target.path(), state_machine.or(before_subcommand).as_deref(), watch)
        }
        Commands::Render { input, format, pretty, no_color, no_metadata, no_content, state_machine } => {
            let target = resolve_plan_target(input)?;
            render_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                format,
                pretty,
                no_color,
                no_metadata,
                no_content,
            )
        }
        Commands::States { input, rhei, json, state_machine } => {
            states_command(input, state_machine.or(before_subcommand).as_deref(), &rhei, json)
        }
        Commands::List {
            input,
            rhei,
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
            state_machine,
        } => {
            let target = resolve_plan_target(input)?;
            let rhei = target.scope_with(&rhei);
            list_command(
            target.path(),
            state_machine.or(before_subcommand).as_deref(),
            ListFilters {
                rhei,
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
            )
        }
        Commands::Transition { input, task, from, to, result, no_callbacks, state_machine } => {
            let (input, task) = split_transition_ticket_target(input, task)?;
            let target = resolve_plan_target(input)?;
            transition_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                &task,
                &from,
                &to,
                result.as_deref(),
                no_callbacks,
            )
        }
        Commands::Run { input, standalone, agent, program, snapshot, state_machine } => {
            let target = resolve_plan_target(input)?;
            let mut opts: RunOptions = (standalone, agent, program, snapshot).into();
            opts.narrow_to(target.scope_with(opts.rhei_scope()));
            run_command(target.path(), state_machine.or(before_subcommand).as_deref(), opts)
        }
        // `rhei cost` reads accounting artifacts under the target's own runtime
        // root and resolves no dependency graph, so it stays on the path it was
        // given rather than widening to the enclosing project.
        Commands::Cost { input, task, json, by } => {
            cost_command(resolve_plan_target(input)?.path(), task.as_deref(), json, by)
        }
        Commands::Intervene { plan, task, slot, message } => {
            intervene_command(&plan, &task, slot, &message)
        }
        Commands::Viz { input, output, open, state_machine } => {
            let target = resolve_plan_target(input)?;
            viz_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                output.as_deref(),
                open,
            )
        }
        Commands::Snapshot { command, state_machine } => snapshot_command(command, state_machine.or(before_subcommand).as_deref()),
        Commands::Templates { template, json, source } => {
            templates::templates_command(json, &source, template.as_deref())
        }
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
        Commands::Next { input, task, json, no_callbacks, peek, rhei, state_machine } => {
            let target = resolve_plan_target(input)?;
            next_command(
                target.path(),
                state_machine.or(before_subcommand).as_deref(),
                task.as_deref(),
                json,
                no_callbacks,
                peek,
                &target.scope_with(&rhei),
            )
        }
        Commands::Complete { input, task, result, no_callbacks, state_machine } => {
            let (input, task) = split_complete_ticket_target(input, task)?;
            let target = resolve_plan_target(input)?;
            complete_command(
                target.path(),
                &target.scope_with(&[]),
                state_machine.or(before_subcommand).as_deref(),
                &task,
                &result,
                no_callbacks,
            )
        }
        Commands::Release { input, task, all, rhei, dry_run, state_machine } => {
            let (input, task) = split_ticket_target(input, task)?;
            let target = resolve_plan_target(input)?;
            release_command(
                target.path(),
                state_machine.or(before_subcommand).as_deref(),
                task.as_deref(),
                all,
                &target.scope_with(&rhei),
                dry_run,
            )
        }
        Commands::Reset { input, rhei, dry_run, yes, state_machine } => {
            // §FS-rhei-panta.6: reset destroys runtime state, so it is the one
            // plan-taking command that never infers an omitted target.
            let Some(input) = input else {
                return Err(miette!(
help = "preview it first: rhei reset <plan-or-project> --dry-run",

                    "`rhei reset` rewrites every in-scope ticket to the initial state \
                     and deletes runtime artifacts, so it never infers its target. \
                     Name the plan or project explicitly: `rhei reset <plan-or-project>`"
                ));
            };
            // Reset never *infers* a target, but an explicit member rhei still
            // loads through its project and narrows to itself. §FS-rhei-panta.6
            let target = resolve_plan_target(Some(input))?;
            reset_command(
                target.path(),
                state_machine.or(before_subcommand).as_deref(),
                &target.scope_with(&rhei),
                dry_run,
                yes,
            )
        }
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
