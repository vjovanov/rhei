use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use fs2::FileExt;
use miette::{miette, Report, Result as MietteResult};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rhei_core::ast::{Metadata, TaskId};
use rhei_core::callback::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
use rhei_core::workspace;
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// Command-line interface for the markdown plan compiler.
#[derive(Parser, Debug)]
#[command(
    name = "rhei",
    author,
    version,
    about = "Validate and compile markdown plans into structured outputs",
    long_about = None,
    arg_required_else_help = true,
    help_template = "\
{about}

Usage: {usage}

Inspection:
  validate    Validate a markdown plan against the configured states
  render      Render a markdown plan into a selected output format
  states      Print the states and allowed transitions for the configured state machine

Execution:
  transition  Atomically transition a task from one state to another (compare-and-swap)
  run         Execute a plan by advancing tasks through the state machine in dependency order
  next        Transition the next ready task to the next state
  complete    Complete a task: transition to terminal state, write result file,\n              link it from the task, and remove the assignee
  reset       Reset all tasks and subtasks to the initial state; for workspaces,\n              also remove runtime output

Setup:
  install-skills  Install rhei skills into AI coding agent configuration directories

Info:
  version     Print versions for the CLI and related crates
  help        Print this message or the help of the given subcommand(s)

Options:
{options}"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Path to a states YAML file (uses built-in default when omitted)"
    )]
    state_machine: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

/// Supported CLI subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Validate a markdown plan against the configured states
    Validate {
        /// Re-run validation when the plan or states file changes
        #[arg(long)]
        watch: bool,
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
    },
    /// Render a markdown plan into a selected output format
    Render {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
        /// Output format
        #[arg(long, value_enum)]
        format: RenderFormat,
        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,
        /// Disable ANSI color in progress output
        #[arg(long)]
        no_color: bool,
        /// Omit metadata in GitHub markdown output
        #[arg(long)]
        no_metadata: bool,
        /// Omit subtask content in GitHub markdown output
        #[arg(long)]
        no_content: bool,
    },
    /// Print the states and allowed transitions for the configured state machine
    States {
        /// Emit the state machine as JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// Atomically transition a task from one state to another (compare-and-swap)
    Transition {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
        /// Task identifier (number or name)
        #[arg(long)]
        task: String,
        /// Expected current state of the task
        #[arg(long)]
        from: String,
        /// Target state to transition to
        #[arg(long)]
        to: String,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Execute a plan by advancing tasks through the state machine in dependency order
    Run {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
        /// Show what transitions would be made without executing them
        #[arg(long)]
        dry_run: bool,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Transition the next ready task to the next state
    ///
    /// Finds the first task whose prerequisites are satisfied, transitions it
    /// forward one step, and prints the task details with state-machine
    /// instructions so an agent knows exactly what to do.
    Next {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
        /// Target a specific task instead of auto-selecting
        #[arg(long)]
        task: Option<String>,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Complete a task: transition to terminal state, write result file,
    /// link it from the task, and remove the assignee.
    Complete {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
        /// Task identifier (number or name)
        #[arg(long)]
        task: String,
        /// Result message written to runtime/results/<task-id>.md
        #[arg(long)]
        result: String,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Reset a plan or workspace to the initial state
    Reset {
        /// Path to the markdown plan file (.rhei.md) or workspace directory
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
    },
    /// Print versions for the CLI and related crates
    Version,
    /// Install rhei skills into AI coding agent configuration directories
    InstallSkills {
        /// Target agent (default: all)
        #[arg(long, value_enum, default_value_t = Agent::All)]
        agent: Agent,
        /// Install into the current project directory instead of global user config
        #[arg(long)]
        local: bool,
        /// Symlink skill files instead of copying
        #[arg(long)]
        link: bool,
        /// Remove previously installed skills
        #[arg(long)]
        uninstall: bool,
        /// Print what would be done without changing anything
        #[arg(long)]
        dry_run: bool,
        /// Comma-separated list of skills to install
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer"
        )]
        skills: Vec<String>,
    },
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

/// Program entry point.
///
/// Delegates to [`run()`](run) so tests can exercise the fallible logic directly.
fn main() {
    if let Err(err) = run() {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}

/// Parse CLI arguments and execute the requested command.
fn run() -> MietteResult<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { watch, input } => {
            validate_command(&input, cli.state_machine.as_deref(), watch)
        }
        Commands::Render { input, format, pretty, no_color, no_metadata, no_content } => {
            render_command(&input, format, pretty, no_color, no_metadata, no_content)
        }
        Commands::States { json } => states_command(cli.state_machine.as_deref(), json),
        Commands::Transition { input, task, from, to, no_callbacks } => transition_command(
            &input,
            cli.state_machine.as_deref(),
            &task,
            &from,
            &to,
            no_callbacks,
        ),
        Commands::Run { input, dry_run, no_callbacks } => {
            run_command(&input, cli.state_machine.as_deref(), dry_run, no_callbacks)
        }
        Commands::Next { input, task, json, no_callbacks } => {
            next_command(&input, cli.state_machine.as_deref(), task.as_deref(), json, no_callbacks)
        }
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
    }
}

/// Load a [`StateMachine`] from the user-provided path, or fall back to the
/// built-in default when no path was given.
fn load_state_machine(path: Option<&Path>) -> MietteResult<rhei_validator::StateMachine> {
    match path {
        Some(p) => rhei_validator::StateMachine::from_yaml_file(p)
            .map_err(|err| file_io_report(p, "failed to load states", err)),
        None => Ok(rhei_validator::StateMachine::builtin_default()),
    }
}

/// Human-readable label for the state machine source, used in diagnostics.
fn state_machine_label(path: Option<&Path>) -> String {
    match path {
        Some(p) => format!("'{}'", p.display()),
        None => "(built-in default)".to_string(),
    }
}

/// Execute the `states` subcommand: load the configured state machine and
/// print its states and declared transitions.
fn states_command(state_machine: Option<&Path>, as_json: bool) -> MietteResult<()> {
    let machine = load_state_machine(state_machine)?;

    if as_json {
        let rendered = render_state_machine_json(&machine)
            .map_err(|err| miette!("failed to serialize state machine: {err}"))?;
        println!("{rendered}");
    } else {
        println!("{}", render_state_machine_text(&machine));
    }

    Ok(())
}

fn render_state_machine_text(machine: &rhei_validator::StateMachine) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "State machine: {} (version: {})\n",
        machine.name,
        format_version(&machine.version)
    ));
    if let Some(personality) =
        machine.personality.as_deref().map(str::trim).filter(|s| !s.is_empty())
    {
        out.push_str(&format!("Personality: {personality}\n"));
    }
    if !machine.models.is_empty() {
        out.push_str(&format!("Models: {}\n", machine.models.join(", ")));
    }

    out.push_str("\nStates:\n");
    if machine.states.is_empty() {
        out.push_str("  (none defined)\n");
    } else {
        for (idx, (name, def)) in machine.states.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            let mut flags = Vec::new();
            if def.initial {
                flags.push("initial");
            }
            if def.terminal {
                flags.push("final");
            }
            let flag_suffix =
                if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join(", ")) };
            let description = def.description.as_deref().unwrap_or("");
            out.push_str(&format!("  {name}{flag_suffix}"));
            if !description.is_empty() {
                out.push_str(&format!(" — {description}"));
            }
            out.push('\n');
            if let Some(iterations) = def.iterations {
                out.push_str(&format!("      Iterations: {iterations}\n"));
            }
            if !def.all_models.is_empty() {
                out.push_str(&format!("      Models: {}\n", def.all_models.join(", ")));
            } else if let Some(model) = def.model.as_deref() {
                out.push_str(&format!("      Model: {model}\n"));
            }
            if let Some(personality) =
                def.personality.as_deref().map(str::trim).filter(|s| !s.is_empty())
            {
                out.push_str(&format!("      Personality: {personality}\n"));
            }
            if let Some(instructions) = def.instructions.as_deref() {
                for line in instructions.lines() {
                    out.push_str(&format!("      {line}\n"));
                }
            }
        }
    }

    out.push_str("\nTransitions:\n");
    if machine.transitions.is_empty() {
        out.push_str("  (none declared)\n");
    } else {
        for rule in &machine.transitions {
            out.push_str(&format!("  {} -> {}", rule.from.0, rule.to.0));
            let mut annotations = Vec::new();
            if let Some(cb) = rule.on_leave.as_ref() {
                annotations.push(format!("on_leave={}", cb.0));
            }
            if let Some(cb) = rule.on_enter.as_ref() {
                annotations.push(format!("on_enter={}", cb.0));
            }
            if let Some(cond) = rule.condition.as_ref() {
                annotations.push(format!("when={cond}"));
            }
            if let Some(t) = rule.timeout.as_ref() {
                annotations.push(format!("timeout={t}"));
            }
            if !annotations.is_empty() {
                out.push_str(&format!(" ({})", annotations.join(", ")));
            }
            out.push('\n');
        }
    }

    out
}

fn render_state_machine_json(machine: &rhei_validator::StateMachine) -> Result<String> {
    let states: Vec<serde_json::Value> = machine
        .states
        .iter()
        .map(|(name, def)| {
            serde_json::json!({
                "name": name,
                "description": def.description,
                "instructions": def.instructions,
                "personality": def.personality,
                "initial": def.initial,
                "final": def.terminal,
                "iterations": def.iterations,
                "all_models": def.all_models,
                "model": def.model,
            })
        })
        .collect();

    let transitions =
        serde_json::to_value(&machine.transitions).context("serialize transitions")?;
    let version =
        serde_json::to_value(&machine.version).context("serialize state machine version")?;

    let payload = serde_json::json!({
        "name": machine.name,
        "models": machine.models,
        "personality": machine.personality,
        "version": version,
        "states": states,
        "transitions": transitions,
    });

    serde_json::to_string_pretty(&payload).context("render state machine as JSON")
}

fn format_version(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        other => serde_yaml::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

/// Read the markdown plan source file from disk.
fn read_input_file(path: &Path) -> MietteResult<String> {
    fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read input file", err))
}

/// A loaded plan with optional workspace task-to-file mapping.
struct LoadedPlan {
    rhei: rhei_core::ast::Rhei,
    /// For directory workspaces: maps task ID string → source file path.
    /// Empty for single-file plans.
    task_sources: HashMap<String, PathBuf>,
}

impl LoadedPlan {
    /// Return the file path that contains the given task.
    /// For single-file plans, returns `fallback` (the plan file itself).
    fn task_file(&self, task_id: &str, fallback: &Path) -> PathBuf {
        self.task_sources.get(task_id).cloned().unwrap_or_else(|| fallback.to_path_buf())
    }
}

/// Load a plan from a file or directory workspace.
fn load_plan(path: &Path) -> MietteResult<LoadedPlan> {
    if workspace::is_workspace(path) {
        let ws = workspace::load_workspace(path).map_err(|err| miette!("{}", err.message))?;
        Ok(LoadedPlan { rhei: ws.rhei, task_sources: ws.task_sources })
    } else {
        let input = read_input_file(path)?;
        let rhei = rhei_core::parse(&input).map_err(|err| parse_report(path, &input, &err))?;
        Ok(LoadedPlan { rhei, task_sources: HashMap::new() })
    }
}

/// Read and parse a markdown plan file into a [`rhei_core::ast::Rhei`](rhei_core::ast::Rhei).
fn parse_input_file(path: &Path) -> MietteResult<rhei_core::ast::Rhei> {
    Ok(load_plan(path)?.rhei)
}

/// Execute the `validate` subcommand once or in watch mode.
fn validate_command(input: &Path, state_machine: Option<&Path>, watch: bool) -> MietteResult<()> {
    if watch {
        watch_validation_command(input, state_machine)
    } else {
        run_validation_once(input, state_machine)
    }
}

/// Parse a plan, load the selected states, and print validation results.
fn run_validation_once(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let rhei = parse_input_file(input)?;
    let machine = load_state_machine(state_machine)?;
    let base_path = input.parent().unwrap_or(Path::new("."));
    let report = rhei_validator::validate_with_machine_and_base(&rhei, &machine, base_path);

    if report.has_errors() {
        return Err(validation_report(input, state_machine, &report.errors));
    }

    print_validation_report(&report.warnings);

    Ok(())
}

/// Print success output and any non-fatal validation warnings.
fn print_validation_report(warnings: &[String]) {
    println!("Validation succeeded");
    for warning in warnings {
        println!("warning: {warning}");
    }
}

/// Watch the plan and states files and re-run validation on relevant changes.
fn watch_validation_command(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let watched_paths = match state_machine {
        Some(sm) => canonical_watched_paths(input, sm),
        None => canonical_watched_paths(input, input), // only watch the plan itself
    };
    let watch_roots = match state_machine {
        Some(sm) => watch_roots(input, sm),
        None => watch_roots(input, input),
    };

    println!(
        "Watch mode started for '{}' (states: {})",
        input.display(),
        state_machine_label(state_machine),
    );

    run_validation_iteration(input, state_machine);

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|err| miette!("failed to initialize file watcher: {err}"))?;

    for root in &watch_roots {
        watcher
            .watch(root, RecursiveMode::NonRecursive)
            .map_err(|err| miette!("failed to watch '{}': {err}", root.display()))?;
    }

    loop {
        let event = match rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
                continue;
            }
            Err(err) => return Err(miette!("watch channel disconnected: {err}")),
        };

        if !should_revalidate(&event, &watched_paths) {
            continue;
        }

        while debounce_has_relevant_event(&rx, &watched_paths) {}

        println!("--- change detected, revalidating ---");
        run_validation_iteration(input, state_machine);
    }
}

/// Run one validation iteration in watch mode, writing any failure to stderr.
fn run_validation_iteration(input: &Path, state_machine: Option<&Path>) {
    if let Err(err) = run_validation_once(input, state_machine) {
        eprintln!("{err:?}");
    }
}

fn debounce_has_relevant_event(
    rx: &mpsc::Receiver<notify::Result<Event>>,
    watched_paths: &[PathBuf],
) -> bool {
    match rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(event)) => should_revalidate(&event, watched_paths),
        Ok(Err(err)) => {
            eprintln!("watch error: {err}");
            false
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

fn should_revalidate(event: &Event, watched_paths: &[PathBuf]) -> bool {
    if !is_relevant_event_kind(&event.kind) {
        return false;
    }

    event.paths.iter().any(|path| path_matches(path, watched_paths))
}

fn is_relevant_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn path_matches(path: &Path, watched_paths: &[PathBuf]) -> bool {
    watched_paths.iter().any(|watched| paths_equivalent(path, watched))
}

fn paths_equivalent(candidate: &Path, watched: &Path) -> bool {
    if let Some(normalized_candidate) = normalize_path(candidate) {
        return normalized_candidate == watched;
    }

    let candidate_file_name = candidate.file_name();
    let watched_file_name = watched.file_name();

    candidate_file_name.is_some()
        && candidate_file_name == watched_file_name
        && candidate.components().last() == watched.components().last()
}

fn canonical_watched_paths(input: &Path, state_machine: &Path) -> Vec<PathBuf> {
    [input, state_machine]
        .into_iter()
        .map(|path| normalize_path(path).unwrap_or_else(|| path.to_path_buf()))
        .collect()
}

fn watch_roots(input: &Path, state_machine: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for path in [input, state_machine] {
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let normalized = normalize_path(root).unwrap_or_else(|| root.to_path_buf());
        if !roots.iter().any(|existing| existing == &normalized) {
            roots.push(normalized);
        }
    }

    roots
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok()
}

fn yaml_key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

fn task_id_yaml_key(task_id: &TaskId) -> YamlValue {
    match task_id {
        TaskId::Number(n) => serde_yaml::to_value(*n).expect("numeric task id should serialize"),
        TaskId::Named(name) => yaml_key(name),
    }
}

fn yaml_u64(value: u64) -> YamlValue {
    serde_yaml::to_value(value).expect("numeric YAML value should serialize")
}

fn yaml_value_to_u64(value: &YamlValue) -> Option<u64> {
    match value {
        YamlValue::Number(number) => number.as_u64(),
        _ => None,
    }
}

fn task_metadata_map<'a>(
    metadata: Option<&'a Metadata>,
    task_id: &TaskId,
) -> Option<&'a YamlMapping> {
    let root = metadata?;
    let metadata_section = root.get(yaml_key("metadata"))?.as_mapping()?;
    let tasks = metadata_section.get(yaml_key("tasks"))?.as_mapping()?;
    tasks.get(task_id_yaml_key(task_id))?.as_mapping()
}

fn task_metadata_number(metadata: Option<&Metadata>, task_id: &TaskId, field: &str) -> Option<u64> {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key(field)))
        .and_then(yaml_value_to_u64)
}

fn task_iteration_count(metadata: Option<&Metadata>, task_id: &TaskId, state_name: &str) -> u64 {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key("stateIterations")))
        .and_then(YamlValue::as_mapping)
        .and_then(|state_iterations| state_iterations.get(yaml_key(state_name)))
        .and_then(yaml_value_to_u64)
        .unwrap_or(0)
}

fn state_iteration_limit(machine: &rhei_validator::StateMachine, state_name: &str) -> Option<u64> {
    machine.states.get(state_name).and_then(|def| def.iterations).map(u64::from)
}

fn resolve_condition_operand(
    token: &str,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<i64> {
    if let Ok(value) = token.parse::<i64>() {
        return Ok(value);
    }

    match token {
        "iterationCount" | "iteration_count" => {
            Ok(task_iteration_count(metadata, task_id, current_state) as i64)
        }
        "iterations" => {
            let limit = state_iteration_limit(machine, current_state).ok_or_else(|| {
                miette!("state '{}' does not declare an iterations limit", current_state)
            })?;
            Ok(limit as i64)
        }
        other => {
            let value = task_metadata_number(metadata, task_id, other).ok_or_else(|| {
                miette!("condition operand '{}' is not available in task metadata", other)
            })?;
            Ok(value as i64)
        }
    }
}

fn evaluate_transition_condition(
    condition: &str,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<bool> {
    let parts = condition.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(miette!(
            "unsupported transition condition '{}'; expected '<lhs> <op> <rhs>'",
            condition
        ));
    }

    let lhs = resolve_condition_operand(parts[0], metadata, task_id, current_state, machine)?;
    let rhs = resolve_condition_operand(parts[2], metadata, task_id, current_state, machine)?;

    let outcome = match parts[1] {
        "<" => lhs < rhs,
        "<=" => lhs <= rhs,
        ">" => lhs > rhs,
        ">=" => lhs >= rhs,
        "==" => lhs == rhs,
        "!=" => lhs != rhs,
        op => {
            return Err(miette!(
                "unsupported operator '{}' in transition condition '{}'",
                op,
                condition
            ))
        }
    };

    Ok(outcome)
}

fn loop_reentry_allowed(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    to_state: &str,
) -> bool {
    let Some(limit) = state_iteration_limit(machine, to_state) else {
        return true;
    };

    let current = task_iteration_count(metadata, task_id, to_state);
    current < limit
}

fn transition_rule_is_applicable(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
) -> MietteResult<bool> {
    if !loop_reentry_allowed(machine, metadata, task_id, &rule.to.0) {
        return Ok(false);
    }

    if let Some(condition) = rule.condition.as_deref() {
        return evaluate_transition_condition(condition, metadata, task_id, current_state, machine);
    }

    Ok(true)
}

fn render_frontmatter_yaml(metadata: &Metadata) -> MietteResult<String> {
    let mut rendered = serde_yaml::to_string(metadata)
        .map_err(|err| miette!("failed to serialize frontmatter: {err}"))?;
    if let Some(stripped) = rendered.strip_prefix("---\n") {
        rendered = stripped.to_string();
    }
    Ok(rendered.trim_end().to_string())
}

fn rewrite_frontmatter(raw: &str, metadata: &Metadata) -> MietteResult<String> {
    let lines = raw.lines().collect::<Vec<_>>();
    let header_index = lines
        .iter()
        .position(|line| line.trim_start().starts_with("# Rhei:"))
        .ok_or_else(|| miette!("could not find '# Rhei:' header when rewriting frontmatter"))?;

    let mut idx = header_index + 1;
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    if idx < lines.len() && lines[idx].trim_start().starts_with("**States:**") {
        idx += 1;
    }
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }

    let start = idx;
    let mut end = idx;
    if start < lines.len() && lines[start].trim() == "---" {
        end += 1;
        while end < lines.len() && lines[end].trim() != "---" {
            end += 1;
        }
        if end == lines.len() {
            return Err(miette!("unterminated YAML frontmatter in plan source"));
        }
        end += 1;
        while end < lines.len() && lines[end].trim().is_empty() {
            end += 1;
        }
    }

    let mut result = Vec::with_capacity(lines.len() + 8);
    result.extend(lines[..start].iter().map(|line| (*line).to_string()));
    result.push("---".to_string());
    let rendered_yaml = render_frontmatter_yaml(metadata)?;
    if !rendered_yaml.is_empty() {
        result.extend(rendered_yaml.lines().map(|line| line.to_string()));
    }
    result.push("---".to_string());
    result.push(String::new());
    result.extend(lines[end..].iter().map(|line| (*line).to_string()));

    let mut output = result.join("\n");
    if raw.ends_with('\n') || !output.is_empty() {
        output.push('\n');
    }
    Ok(output)
}

fn ensure_mapping(parent: &mut YamlMapping, key: YamlValue) -> &mut YamlMapping {
    if !matches!(parent.get(&key), Some(YamlValue::Mapping(_))) {
        parent.insert(key.clone(), YamlValue::Mapping(YamlMapping::new()));
    }

    match parent.get_mut(&key) {
        Some(YamlValue::Mapping(mapping)) => mapping,
        _ => unreachable!("mapping just initialized"),
    }
}

fn update_metadata_for_transition(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    to_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<Metadata> {
    state_iteration_limit(machine, to_state)?;

    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let state_iterations = ensure_mapping(task_entry, yaml_key("stateIterations"));
    let state_key = yaml_key(to_state);
    let next =
        state_iterations.get(&state_key).and_then(yaml_value_to_u64).map(|n| n + 1).unwrap_or(0);
    state_iterations.insert(state_key, yaml_u64(next));
    Some(root)
}

fn clear_runtime_state_iterations(existing: Option<&Metadata>) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata_section)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata_section.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };

    for value in tasks.values_mut() {
        if let YamlValue::Mapping(task_map) = value {
            task_map.remove(yaml_key("stateIterations"));
        }
    }

    Some(root)
}

struct CallbackPaths {
    plan_path: PathBuf,
    working_dir: PathBuf,
}

#[derive(Clone, Copy)]
struct TransitionFiles<'a> {
    task_file: &'a Path,
    metadata_file: &'a Path,
}

fn resolve_callback_paths(
    state_machine_path: Option<&Path>,
    plan_path: &Path,
) -> MietteResult<CallbackPaths> {
    let plan_path = plan_path.canonicalize().map_err(|err| {
        file_io_report(plan_path, "failed to resolve plan path for callbacks", err)
    })?;
    let base_dir = if let Some(path) = state_machine_path {
        path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
    } else if plan_path.is_dir() {
        plan_path.as_path()
    } else {
        plan_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
    };

    let working_dir = base_dir.canonicalize().map_err(|err| {
        file_io_report(base_dir, "failed to resolve callback working directory", err)
    })?;

    Ok(CallbackPaths { plan_path, working_dir })
}

/// Execute the `transition` subcommand: atomic compare-and-swap state change.
///
/// Acquires an exclusive file lock, verifies the task's current state matches
/// `from`, validates the transition against the state machine, rewrites the
/// `**State:**` line, and writes the file atomically (temp + rename).
fn transition_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let machine = load_state_machine(state_machine_path)?;
    let callback_paths = resolve_callback_paths(state_machine_path, input)?;

    let task_file = if workspace::is_workspace(input) {
        let loaded = load_plan(input)?;
        loaded.task_file(task_id_str, input)
    } else {
        input.to_path_buf()
    };
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };

    execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        &callback_paths,
        &machine,
        task_id_str,
        from,
        to,
        no_callbacks,
    )?;

    let root = result_workspace_root(input, &task_file);
    append_result_entry(&root, task_id_str, from, to, None)?;

    println!("Task {} transitioned: '{}' → '{}'", task_id_str, from, to);
    Ok(())
}

/// Core transition logic shared by `transition` and `run` commands.
///
/// Validates states and transition legality, acquires an exclusive file lock,
/// performs compare-and-swap verification, executes callbacks, and atomically
/// rewrites the plan file. Returns an error if any step fails.
///
/// `task_file` is the specific file to lock and rewrite (for directory
/// workspaces this is the file inside `tasks/` that contains the task;
/// for single-file plans it equals `plan_path`).
///
/// `plan_path` is the top-level plan path used in callback context.
fn execute_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let task_file = files.task_file;
    let metadata_file = files.metadata_file;

    // Validate that both `from` and `to` are valid states.
    if !machine.is_valid_state(from) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!("'{}' is not a valid state. Allowed: [{}]", from, allowed));
    }
    if !machine.is_valid_state(to) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!("'{}' is not a valid state. Allowed: [{}]", to, allowed));
    }

    let matching_rule =
        machine.transitions().iter().find(|rule| rule.from.0 == from && rule.to.0 == to).or_else(
            || machine.transitions().iter().find(|rule| rule.from.0 == "*" && rule.to.0 == to),
        );
    let Some(matching_rule) = matching_rule else {
        return Err(miette!(
            "transition from '{}' to '{}' is not allowed by the state machine",
            from,
            to
        ));
    };

    // Open the file(s) with an exclusive lock for the duration of the operation.
    let metadata_handle = fs::File::open(metadata_file)
        .map_err(|err| file_io_report(metadata_file, "failed to open plan file", err))?;
    metadata_handle
        .lock_exclusive()
        .map_err(|err| file_io_report(metadata_file, "failed to acquire file lock", err))?;
    let task_handle = if task_file == metadata_file {
        None
    } else {
        let handle = fs::File::open(task_file)
            .map_err(|err| file_io_report(task_file, "failed to open plan file", err))?;
        handle
            .lock_exclusive()
            .map_err(|err| file_io_report(task_file, "failed to acquire file lock", err))?;
        Some(handle)
    };

    // Read the raw markdown while holding the locks.
    let metadata_raw = fs::read_to_string(metadata_file)
        .map_err(|err| file_io_report(metadata_file, "failed to read plan file", err))?;
    let task_raw = if task_file == metadata_file {
        metadata_raw.clone()
    } else {
        fs::read_to_string(task_file)
            .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?
    };

    // Parse to validate structure and find the task.
    // Try full plan parse first; fall back to workspace task-file parse.
    let target_id = parse_task_id(task_id_str);
    let current_state = find_task_current_state(&task_raw, task_file, &target_id, task_id_str)?;
    let metadata = if task_file == metadata_file {
        rhei_core::parse(&metadata_raw)
            .map_err(|err| {
                miette!("failed to parse plan for transition metadata: {}", err.message)
            })?
            .metadata
    } else {
        rhei_core::parser::parse_workspace_index(&metadata_raw)
            .map_err(|err| {
                miette!("failed to parse workspace index for transition metadata: {}", err.message)
            })?
            .metadata
    };

    // Compare-and-swap: verify the task's current state matches `from`.
    if current_state != from {
        if let Some(task_handle) = &task_handle {
            let _ = task_handle.unlock();
        }
        let _ = metadata_handle.unlock();
        return Err(miette!(
            "conflict: Task {} is in state '{}', expected '{}'",
            task_id_str,
            current_state,
            from
        ));
    }

    if !transition_rule_is_applicable(matching_rule, machine, metadata.as_ref(), &target_id, from)?
    {
        if let Some(task_handle) = &task_handle {
            let _ = task_handle.unlock();
        }
        let _ = metadata_handle.unlock();
        return Err(miette!(
            "transition from '{}' to '{}' is blocked by loop budget or condition",
            from,
            to
        ));
    }

    // When the FROM state declares all_models, run on_leave once per model; otherwise run once
    // without a model (None).
    let from_models: Vec<Option<String>> = machine
        .states
        .get(from)
        .filter(|s| !s.all_models.is_empty())
        .map(|s| s.all_models.iter().map(|m| Some(m.clone())).collect())
        .unwrap_or_else(|| vec![None]);

    // Execute on_leave callback before the state change.
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_leave {
            let executor = ShellCallbackExecutor;
            for model in &from_models {
                let ctx = CallbackContext {
                    task_id: task_id_str,
                    from_state: from,
                    to_state: to,
                    plan_path: &callback_paths.plan_path,
                    callback_cwd: &callback_paths.working_dir,
                    model: model.as_deref(),
                };
                let result = executor.execute(cb, &ctx).map_err(|e| miette!("{e}"))?;
                if !result.success {
                    if let Some(task_handle) = &task_handle {
                        let _ = task_handle.unlock();
                    }
                    let _ = metadata_handle.unlock();
                    let stderr = result.stderr.trim();
                    let detail =
                        if stderr.is_empty() { String::new() } else { format!(": {stderr}") };
                    return Err(miette!(
                        "on_leave callback '{}' rejected the transition{detail}",
                        cb.0
                    ));
                }
            }
        }
    }

    let updated_metadata =
        update_metadata_for_transition(metadata.as_ref(), &target_id, to, machine);
    let metadata_raw_updated = if task_file == metadata_file {
        let new_task_raw = rewrite_task_state(&task_raw, task_id_str, to)?;
        if let Some(updated_metadata) = updated_metadata.as_ref() {
            rewrite_frontmatter(&new_task_raw, updated_metadata)?
        } else {
            new_task_raw
        }
    } else if let Some(updated_metadata) = updated_metadata.as_ref() {
        rewrite_frontmatter(&metadata_raw, updated_metadata)?
    } else {
        metadata_raw.clone()
    };

    let task_raw_updated = if task_file == metadata_file {
        None
    } else {
        Some(rewrite_task_state(&task_raw, task_id_str, to)?)
    };

    // Atomic write(s): write to temp file in the same directory, then rename.
    let metadata_parent = metadata_file.parent().unwrap_or(Path::new("."));
    let mut metadata_tmp = tempfile::NamedTempFile::new_in(metadata_parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    metadata_tmp
        .write_all(metadata_raw_updated.as_bytes())
        .map_err(|err| miette!("failed to write temp file: {err}"))?;
    metadata_tmp
        .persist(metadata_file)
        .map_err(|err| miette!("failed to persist temp file: {err}"))?;

    if let Some(task_raw_updated) = task_raw_updated {
        let task_parent = task_file.parent().unwrap_or(Path::new("."));
        let mut task_tmp = tempfile::NamedTempFile::new_in(task_parent)
            .map_err(|err| miette!("failed to create temp file: {err}"))?;
        task_tmp
            .write_all(task_raw_updated.as_bytes())
            .map_err(|err| miette!("failed to write temp file: {err}"))?;
        task_tmp.persist(task_file).map_err(|err| miette!("failed to persist temp file: {err}"))?;
    }

    // Execute on_enter callback after the state change (not model-looped).
    let callback_ctx = CallbackContext {
        task_id: task_id_str,
        from_state: from,
        to_state: to,
        plan_path: &callback_paths.plan_path,
        callback_cwd: &callback_paths.working_dir,
        model: None,
    };
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_enter {
            let executor = ShellCallbackExecutor;
            let result = executor.execute(cb, &callback_ctx).map_err(|e| miette!("{e}"))?;
            if !result.success {
                let stderr = result.stderr.trim();
                let detail = if stderr.is_empty() { String::new() } else { format!(": {stderr}") };
                eprintln!(
                    "warning: on_enter callback '{}' failed after state change{detail}",
                    cb.0
                );
            }
        }
    }

    if let Some(task_handle) = task_handle {
        let _ = task_handle.unlock();
    }
    let _ = metadata_handle.unlock();
    Ok(())
}

/// Extract the current state of a task from raw markdown content.
///
/// Tries full-plan parsing first, falls back to workspace task-file parsing.
fn find_task_current_state(
    raw: &str,
    file_path: &Path,
    target_id: &TaskId,
    task_id_str: &str,
) -> MietteResult<String> {
    // Try full plan parse.
    if let Ok(rhei) = rhei_core::parse(raw) {
        if let Some(task) = rhei.tasks.iter().find(|t| &t.id == target_id) {
            return Ok(task.state.as_str().to_string());
        }
    }

    // Try workspace task-file parse.
    if let Ok(tasks) = rhei_core::parser::parse_workspace_tasks(raw) {
        if let Some(task) = tasks.iter().find(|t| &t.id == target_id) {
            return Ok(task.state.as_str().to_string());
        }
    }

    Err(miette!("task '{}' not found in {}", task_id_str, file_path.display()))
}

/// Execute the `run` subcommand: advance tasks through the state machine
/// in dependency order.
///
/// Walks the task DAG in topological order, identifies tasks whose
/// prerequisites are all in terminal states, finds the next valid transition
/// for each ready task, and executes it. Repeats until no more progress
/// can be made.
fn run_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    dry_run: bool,
    no_callbacks: bool,
) -> MietteResult<()> {
    let machine = load_state_machine(state_machine_path)?;
    let callback_paths = resolve_callback_paths(state_machine_path, input)?;

    // Initial validation pass.
    let loaded = load_plan(input)?;
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, state_machine_path, &report.errors));
    }

    let mut transitions_made = 0u32;

    loop {
        // Re-load the plan each iteration to pick up changes.
        let loaded = load_plan(input)?;

        // Find tasks that are ready to advance.
        let ready = find_ready_tasks(&loaded.rhei, &machine);
        if ready.is_empty() {
            break;
        }

        let mut advanced_any = false;

        for task in &ready {
            let task_id_str = task.id.to_string();
            let current_state = task.state.as_str();
            // Find the next forward transition (explicit from-state match, not wildcard).
            let next_to = find_next_transition(task, &loaded.rhei, &machine)?;

            let Some(to_state) = next_to else {
                continue;
            };

            if dry_run {
                println!(
                    "Would transition Task {} from '{}' to '{}'",
                    task_id_str, current_state, to_state
                );
                continue;
            }

            let task_file = loaded.task_file(&task_id_str, input);
            let metadata_file = if workspace::is_workspace(input) {
                input.join("index.rhei.md")
            } else {
                task_file.clone()
            };
            match execute_transition(
                TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
                &callback_paths,
                &machine,
                &task_id_str,
                current_state,
                &to_state,
                no_callbacks,
            ) {
                Ok(()) => {
                    println!(
                        "Task {} transitioned: '{}' → '{}'",
                        task_id_str, current_state, to_state
                    );
                    transitions_made += 1;
                    advanced_any = true;
                    // Re-read after each transition to see updated state.
                    break;
                }
                Err(err) => {
                    eprintln!("warning: failed to advance Task {}: {}", task_id_str, err);
                    continue;
                }
            }
        }

        if dry_run || !advanced_any {
            break;
        }
    }

    // Print summary.
    if dry_run {
        println!("\nDry run complete — no changes were made.");
    } else if transitions_made == 0 {
        println!("No tasks could be advanced.");
    } else {
        // Re-load for final summary.
        let loaded = load_plan(input)?;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|t| is_terminal_state(t.state.as_str(), &machine))
            .count();
        println!(
            "\nRun complete: {} transition(s) made, {}/{} tasks in terminal state.",
            transitions_made,
            terminal_count,
            loaded.rhei.tasks.len()
        );
    }

    Ok(())
}

/// Check whether a dependency state satisfies a prerequisite edge.
///
/// Terminal cancellation does not satisfy dependencies: a cancelled task should
/// not unblock downstream work.
fn dependency_is_satisfied(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    state != "cancelled" && is_terminal_state(state, machine)
}

/// Find tasks that are ready to advance: not in a terminal state and all
/// prior dependencies are satisfied.
///
/// Returns task references in source order.
fn find_ready_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> Vec<&'a rhei_core::ast::Task> {
    use std::collections::HashMap;

    // Build a map of task id → current state for dependency lookups.
    let state_map: HashMap<&TaskId, &str> =
        rhei.tasks.iter().map(|t| (&t.id, t.state.as_str())).collect();

    let mut ready = Vec::new();

    for task in &rhei.tasks {
        let current_state = task.state.as_str();

        // Skip tasks already in a terminal state.
        if is_terminal_state(current_state, machine) {
            continue;
        }

        // Check that all prior dependencies are satisfied.
        let all_priors_done = task.prior.iter().all(|dep_id| {
            state_map.get(dep_id).map(|s| dependency_is_satisfied(s, machine)).unwrap_or(false)
        });

        if all_priors_done {
            ready.push(task);
        }
    }

    ready
}

/// Find tasks that are ready to be claimed by `rhei next` in automatic mode.
///
/// Only tasks in the initial state are claimable automatically. Already-claimed
/// work can still be inspected with `rhei next --task <id>`.
fn find_claimable_tasks<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> Vec<&'a rhei_core::ast::Task> {
    find_ready_tasks(rhei, machine)
        .into_iter()
        .filter(|task| {
            machine.states.get(task.state.as_str()).map(|def| def.initial).unwrap_or(false)
        })
        .collect()
}

/// Check whether a state is terminal (final) in the state machine.
fn is_terminal_state(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    machine.states.get(state).map(|def| def.terminal).unwrap_or(false)
}

/// Find the next forward transition from a given state.
///
/// Prefers exact `from` matches over wildcard (`*`) rules, and skips
/// transitions to terminal states via wildcards (those are escape hatches
/// like cancellation, not forward progress).
fn find_next_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<Option<String>> {
    let current_state = task.state.as_str();

    // First, look for an exact from-state match.
    for rule in machine.transitions() {
        if rule.from.0 == current_state
            && transition_rule_is_applicable(
                rule,
                machine,
                rhei.metadata.as_ref(),
                &task.id,
                current_state,
            )?
        {
            return Ok(Some(rule.to.0.clone()));
        }
    }

    // Fall back to wildcard, but only to non-terminal states (forward progress).
    for rule in machine.transitions() {
        if rule.from.0 == "*" {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if !is_terminal
                && transition_rule_is_applicable(
                    rule,
                    machine,
                    rhei.metadata.as_ref(),
                    &task.id,
                    current_state,
                )?
            {
                return Ok(Some(rule.to.0.clone()));
            }
        }
    }

    Ok(None)
}

/// Parse a task ID string into a [`TaskId`].
fn parse_task_id(s: &str) -> TaskId {
    match s.parse::<u32>() {
        Ok(n) => TaskId::Number(n),
        Err(_) => TaskId::Named(s.to_string()),
    }
}

/// Rewrite the `**State:**` line for a specific task in the raw markdown.
///
/// Locates the `### Task <id>:` header and replaces the immediately following
/// `**State:**` line with the new state value.
fn rewrite_task_state(raw: &str, task_id: &str, new_state: &str) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::with_capacity(lines.len());

    // Build the task header prefix to match.
    let task_prefix = format!("### Task {}:", task_id);

    let mut in_target_task = false;
    let mut state_replaced = false;

    for line in &lines {
        if !state_replaced && line.starts_with("### Task ") {
            in_target_task = line.starts_with(&task_prefix);
        }

        if in_target_task && !state_replaced && line.starts_with("**State:**") {
            // Format the new state: use backtick quoting if it contains spaces.
            let formatted = if new_state.contains(' ') {
                format!("**State:** `{}`", new_state)
            } else {
                format!("**State:** {}", new_state)
            };
            result.push(formatted);
            state_replaced = true;
            continue;
        }

        result.push(line.to_string());
    }

    if !state_replaced {
        return Err(miette!("could not find **State:** line for Task {} in the markdown", task_id));
    }

    // Preserve trailing newline if original had one.
    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Execute the `next` subcommand: transition the next ready task to the next state,
/// and print the task details with instructions.
fn next_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_filter: Option<&str>,
    as_json: bool,
    no_callbacks: bool,
) -> MietteResult<()> {
    let machine = load_state_machine(state_machine_path)?;
    let callback_paths = resolve_callback_paths(state_machine_path, input)?;

    // Validate the plan first.
    let loaded = load_plan(input)?;
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, state_machine_path, &report.errors));
    }

    // Find the target task to claim.
    let (task_id_str, current_state) = if let Some(tid) = task_id_filter {
        let target_id = parse_task_id(tid);
        let task = loaded
            .rhei
            .tasks
            .iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| miette!("task '{}' not found in the plan", tid))?;
        let is_initial =
            machine.states.get(task.state.as_str()).map(|def| def.initial).unwrap_or(false);
        if is_initial {
            let state_map: HashMap<&TaskId, &str> =
                loaded.rhei.tasks.iter().map(|t| (&t.id, t.state.as_str())).collect();
            let all_priors_done = task.prior.iter().all(|dep_id| {
                state_map.get(dep_id).map(|s| dependency_is_satisfied(s, &machine)).unwrap_or(false)
            });
            if !all_priors_done {
                return Err(miette!("Task {} is blocked by incomplete prerequisites", tid));
            }
        }
        let state = task.state.as_str().to_string();
        (tid.to_string(), state)
    } else {
        let ready = find_claimable_tasks(&loaded.rhei, &machine);
        if ready.is_empty() {
            return Err(miette!("no tasks are ready to claim"));
        }
        let task = ready.into_iter().next().unwrap();
        (task.id.to_string(), task.state.to_string())
    };

    // Determine whether we need a state transition.
    // Tasks in an initial state (e.g. draft) are transitioned forward.
    let is_initial = machine.states.get(&current_state).map(|d| d.initial).unwrap_or(false);

    let task_file = loaded.task_file(&task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };

    let final_state = if is_initial {
        // Advance from the initial state (e.g. draft → pending).
        let target_id = parse_task_id(&task_id_str);
        let task = loaded
            .rhei
            .tasks
            .iter()
            .find(|task| task.id == target_id)
            .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
        let to_state = find_next_transition(task, &loaded.rhei, &machine)?.ok_or_else(|| {
            miette!("no forward transition available from state '{}'", current_state)
        })?;
        execute_transition(
            TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
            &callback_paths,
            &machine,
            &task_id_str,
            &current_state,
            &to_state,
            no_callbacks,
        )?;
        to_state
    } else {
        current_state.to_string()
    };

    // Re-load to get the updated task for output.
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&task_id_str);
    let task = loaded
        .rhei
        .tasks
        .iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| miette!("task '{}' not found after transition", task_id_str))?;

    let instructions = state_instructions(&machine, &final_state);
    let personality = machine
        .states
        .get(final_state.as_str())
        .and_then(|def| def.personality.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| machine.personality.as_deref().map(str::trim).filter(|s| !s.is_empty()));
    print_next_output(as_json, task, &current_state, &final_state, personality, &instructions);

    Ok(())
}

/// Execute the `complete` subcommand: transition a task to a terminal state,
/// write the result to `runtime/results/<task-id>.md`, link it from the task
/// body, and remove the assignee.
///
/// The target terminal state is chosen automatically: the first non-cancelled
/// terminal state reachable from the task's current state via a declared
/// transition. If no such transition exists, the command fails.
fn complete_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    result_msg: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let machine = load_state_machine(state_machine_path)?;
    let callback_paths = resolve_callback_paths(state_machine_path, input)?;

    // Validate the plan first.
    let loaded = load_plan(input)?;
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, state_machine_path, &report.errors));
    }

    // Find the task and its current state.
    let target_id = parse_task_id(task_id_str);
    let task = loaded
        .rhei
        .tasks
        .iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
    let current_state = task.state.as_str();

    // Reject tasks already in a terminal state.
    if is_terminal_state(current_state, &machine) {
        return Err(miette!(
            "Task {} is already in terminal state '{}'",
            task_id_str,
            current_state
        ));
    }

    // Find the completion target: a non-cancelled terminal state reachable via
    // a single declared transition from the current state.
    let to_state = find_completion_state(current_state, &machine).ok_or_else(|| {
        miette!(
            "no transition to a terminal state available from '{}' for Task {}",
            current_state,
            task_id_str
        )
    })?;

    // Execute the state transition (compare-and-swap, callbacks, atomic write).
    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };
    execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        &callback_paths,
        &machine,
        task_id_str,
        current_state,
        &to_state,
        no_callbacks,
    )?;

    // Append the completion entry to the result file.
    let root = result_workspace_root(input, &task_file);
    let result_link = format!("runtime/results/{}.md", task_id_str);
    let result_file_existed = root.join(&result_link).exists();
    append_result_entry(&root, task_id_str, current_state, &to_state, Some(result_msg))?;

    // Post-transition: remove assignee and link the result file (first time only).
    rewrite_task_completion(
        &task_file,
        task_id_str,
        task_id_str,
        &result_link,
        !result_file_existed,
    )?;

    println!(
        "Task {} completed: '{}' → '{}' ({})",
        task_id_str, current_state, to_state, result_link
    );

    Ok(())
}

/// Execute the `reset` subcommand: restore every task and subtask to the
/// state machine's initial state.
///
/// For directory workspaces, this also removes the generated `runtime/`
/// directory so logs and artifacts do not survive the reset.
fn reset_command(input: &Path, state_machine_path: Option<&Path>) -> MietteResult<()> {
    let machine = load_state_machine(state_machine_path)?;
    let initial_state = initial_state_name(&machine)?;
    let loaded = load_plan(input)?;
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, state_machine_path, &report.errors));
    }

    let task_count = loaded.rhei.tasks.len();
    let subtask_count = loaded.rhei.tasks.iter().map(|task| task.subtasks.len()).sum::<usize>();

    for file in reset_target_files(&loaded, input) {
        reset_plan_file_states(&file, &initial_state)?;
    }
    if workspace::is_workspace(input) {
        clear_runtime_metadata_in_file(&input.join("index.rhei.md"), true)?;
    }

    let mut removed_runtime = false;
    if workspace::is_workspace(input) {
        let runtime_dir = input.join("runtime");
        if runtime_dir.exists() {
            fs::remove_dir_all(&runtime_dir).map_err(|err| {
                file_io_report(&runtime_dir, "failed to remove runtime directory", err)
            })?;
            removed_runtime = true;
        }
    }

    println!(
        "Reset {} task(s) and {} subtask(s) to initial state '{}'.",
        task_count, subtask_count, initial_state
    );
    if workspace::is_workspace(input) {
        if removed_runtime {
            println!("Removed workspace runtime output.");
        } else {
            println!("No workspace runtime output was present.");
        }
    }

    Ok(())
}

fn initial_state_name(machine: &rhei_validator::StateMachine) -> MietteResult<String> {
    let initial_states = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    match initial_states.as_slice() {
        [] => Err(miette!("state machine '{}' does not declare an initial state", machine.name)),
        [initial] => Ok(initial.clone()),
        many => Err(miette!(
            "state machine '{}' declares multiple initial states: {}",
            machine.name,
            many.join(", ")
        )),
    }
}

fn reset_target_files(loaded: &LoadedPlan, input: &Path) -> Vec<PathBuf> {
    if loaded.task_sources.is_empty() {
        return vec![input.to_path_buf()];
    }

    let mut files = loaded.task_sources.values().cloned().collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

fn reset_plan_file_states(path: &Path, initial_state: &str) -> MietteResult<()> {
    let file = fs::File::open(path)
        .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
    file.lock_exclusive()
        .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;

    let raw = fs::read_to_string(path)
        .map_err(|err| file_io_report(path, "failed to read plan file", err))?;
    let new_raw = rewrite_all_states_to_initial(&raw, initial_state)?;
    let new_raw = match rhei_core::parse(&new_raw) {
        Ok(rhei) => {
            if let Some(metadata) = clear_runtime_state_iterations(rhei.metadata.as_ref()) {
                rewrite_frontmatter(&new_raw, &metadata)?
            } else {
                new_raw
            }
        }
        Err(_) => new_raw,
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(path).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    let _ = file.unlock();
    Ok(())
}

fn clear_runtime_metadata_in_file(path: &Path, workspace_index: bool) -> MietteResult<()> {
    let file = fs::File::open(path)
        .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
    file.lock_exclusive()
        .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;

    let raw = fs::read_to_string(path)
        .map_err(|err| file_io_report(path, "failed to read plan file", err))?;
    let metadata = if workspace_index {
        rhei_core::parser::parse_workspace_index(&raw)
            .map_err(|err| {
                miette!("failed to parse workspace index for metadata reset: {}", err.message)
            })?
            .metadata
    } else {
        rhei_core::parse(&raw)
            .map_err(|err| miette!("failed to parse plan for metadata reset: {}", err.message))?
            .metadata
    };

    let new_raw = if let Some(metadata) = clear_runtime_state_iterations(metadata.as_ref()) {
        rewrite_frontmatter(&raw, &metadata)?
    } else {
        raw
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(path).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    let _ = file.unlock();
    Ok(())
}

fn rewrite_all_states_to_initial(raw: &str, initial_state: &str) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut expecting_state = false;
    let mut rewrites = 0usize;

    for line in &lines {
        if line.starts_with("### Task ") || line.starts_with("#### Subtask ") {
            if expecting_state {
                return Err(miette!("could not find **State:** line before the next task header"));
            }
            expecting_state = true;
            result.push((*line).to_string());
            continue;
        }

        if expecting_state && line.starts_with("**State:**") {
            let formatted = if initial_state.contains(' ') {
                format!("**State:** `{initial_state}`")
            } else {
                format!("**State:** {initial_state}")
            };
            result.push(formatted);
            expecting_state = false;
            rewrites += 1;
            continue;
        }

        result.push((*line).to_string());
    }

    if expecting_state {
        return Err(miette!("could not find **State:** line at the end of the plan"));
    }
    if rewrites == 0 {
        return Err(miette!("found no task state metadata to reset"));
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Find a terminal (non-cancelled) state reachable in one transition.
///
/// Prefers exact `from` matches over wildcards. Cancellation is not considered
/// a completion target for `rhei complete`.
fn find_completion_state(
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<String> {
    // Exact from-state matches first.
    for rule in machine.transitions() {
        if rule.from.0 == current_state {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal && rule.to.0 != "cancelled" {
                return Some(rule.to.0.clone());
            }
        }
    }

    // Fall back to wildcard transitions.
    for rule in machine.transitions() {
        if rule.from.0 == "*" {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal && rule.to.0 != "cancelled" {
                return Some(rule.to.0.clone());
            }
        }
    }

    None
}

/// Resolve the workspace root for result file placement.
fn result_workspace_root(input: &Path, task_file: &Path) -> PathBuf {
    if workspace::is_workspace(input) {
        input.to_path_buf()
    } else {
        task_file.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}

/// Append a state-transition entry to `runtime/results/<task-id>.md`.
///
/// Each entry is a markdown heading (`## from → to`) optionally followed by
/// a message body.  The file is created (with directories) on the first call.
fn append_result_entry(
    workspace_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
    message: Option<&str>,
) -> MietteResult<()> {
    let results_dir = workspace_root.join("runtime").join("results");
    fs::create_dir_all(&results_dir)
        .map_err(|err| miette!("failed to create runtime/results directory: {err}"))?;
    let result_file = results_dir.join(format!("{}.md", task_id));

    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&result_file)
        .map_err(|err| miette!("failed to open result file: {err}"))?;

    writeln!(file, "## {} \u{2192} {}", from, to)
        .map_err(|err| miette!("failed to write result entry: {err}"))?;
    if let Some(msg) = message {
        writeln!(file).map_err(|err| miette!("failed to write result entry: {err}"))?;
        writeln!(file, "{}", msg).map_err(|err| miette!("failed to write result entry: {err}"))?;
    }
    writeln!(file).map_err(|err| miette!("failed to write result entry: {err}"))?;

    Ok(())
}

/// Rewrite a task's markdown after completion: remove `**Assignee:**` and,
/// when `insert_link` is true, append a `> **Result:** [link_text](link_path)`
/// line to the task body.
///
/// Operates on raw text lines so the parser does not need to know about
/// assignee or result fields.
fn rewrite_task_completion(
    task_file: &Path,
    task_id: &str,
    link_text: &str,
    link_path: &str,
    insert_link: bool,
) -> MietteResult<()> {
    let raw = fs::read_to_string(task_file)
        .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?;

    let lines: Vec<&str> = raw.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let task_prefix = format!("### Task {}:", task_id);

    let mut in_target_task = false;
    let mut link_inserted = !insert_link; // skip insertion when not requested
    let result_line = format!("> **Result:** [{}]({})", link_text, link_path);

    for line in &lines {
        let is_new_task = line.starts_with("### Task ") && !line.starts_with(&task_prefix);
        let is_subtask = line.starts_with("#### Subtask ");

        // When we hit a new structural element while still inside the target
        // task, insert the result link before that element.
        if in_target_task && !link_inserted && (is_new_task || is_subtask) {
            result_lines.push(String::new());
            result_lines.push(result_line.clone());
            link_inserted = true;
        }

        if line.starts_with("### Task ") {
            in_target_task = line.starts_with(&task_prefix);
        }

        // Strip the assignee line from the target task.
        if in_target_task && line.starts_with("**Assignee:**") {
            continue;
        }

        result_lines.push(line.to_string());
    }

    // If the target task is the last element in the file, append here.
    if in_target_task && !link_inserted {
        result_lines.push(String::new());
        result_lines.push(result_line);
    }

    let mut output = result_lines.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }

    // Atomic write.
    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(output.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(task_file).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    Ok(())
}

/// Get the instructions text for a given state from the state machine.
fn state_instructions(machine: &rhei_validator::StateMachine, state: &str) -> String {
    machine
        .states
        .get(state)
        .and_then(|def| def.instructions.as_deref())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Print the `next` command output in either human-readable or JSON format.
fn print_next_output(
    as_json: bool,
    task: &rhei_core::ast::Task,
    from_state: &str,
    to_state: &str,
    personality: Option<&str>,
    instructions: &str,
) {
    if as_json {
        let subtasks: Vec<serde_json::Value> = task
            .subtasks
            .iter()
            .map(|st| {
                serde_json::json!({
                    "id": format!("{}.{}", st.task_number, st.subtask_number),
                    "title": st.title,
                    "state": st.state,
                    "content": st.content.trim(),
                })
            })
            .collect();

        let obj = serde_json::json!({
            "task_id": task.id.to_string(),
            "title": task.title,
            "from_state": from_state,
            "state": to_state,
            "personality": personality,
            "instructions": instructions,
            "content": task.content.trim(),
            "subtasks": subtasks,
        });
        println!("{}", serde_json::to_string_pretty(&obj).expect("JSON serialization"));
    } else {
        let transitioned = from_state != to_state;
        if transitioned {
            println!("Task {} claimed: '{}' → '{}'", task.id, from_state, to_state);
        } else {
            println!("Task {} (already in '{}')", task.id, to_state);
        }
        if let Some(personality) = personality {
            println!();
            println!("Personality: {}", personality);
        }
        println!();
        println!("## Task {}: {}", task.id, task.title);
        if !task.content.trim().is_empty() {
            println!();
            println!("{}", task.content.trim());
        }
        if !task.subtasks.is_empty() {
            println!();
            for st in &task.subtasks {
                let st_state = &st.state;
                println!(
                    "  - {}.{}: {} [{}]",
                    st.task_number, st.subtask_number, st.title, st_state
                );
                if !st.content.trim().is_empty() {
                    for line in st.content.trim().lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
        if !instructions.is_empty() {
            println!();
            println!("--- Instructions ({}) ---", to_state);
            println!("{}", instructions);
        }
    }
}

/// Execute the `render` subcommand for the selected output format.
fn render_command(
    input: &Path,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> MietteResult<()> {
    let rhei = parse_input_file(input)?;
    let rendered = render_rhei(&rhei, format, pretty, no_color, no_metadata, no_content)
        .map_err(|err| miette!("{err}"))?;
    println!("{rendered}");
    Ok(())
}

/// Render a parsed rhei into the requested output representation.
fn render_rhei(
    rhei: &rhei_core::ast::Rhei,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> Result<String> {
    match format {
        RenderFormat::Json => {
            if pretty {
                Ok(rhei_output::to_json_string_pretty(rhei))
            } else {
                let value = rhei_output::to_json_value(rhei);
                serde_json::to_string(&value).context("failed to serialize JSON output")
            }
        }
        RenderFormat::Github => Ok(rhei_output::GithubIssuesOutput {
            include_content: !no_content,
            include_metadata: !no_metadata,
        }
        .to_markdown(rhei)),
        RenderFormat::Progress => {
            Ok(rhei_output::ProgressReportOutput { color: !no_color, show_dependencies: true }
                .to_string(rhei))
        }
    }
}

/// Print versions for the CLI and the crates surfaced by this command.
fn print_versions() {
    println!("rhei-cli {}", env!("CARGO_PKG_VERSION"));
    println!("rhei-core {}", rhei_core::version());
    println!("rhei-validator {}", rhei_validator::version());
    println!("rhei-output {}", rhei_output::version());
}

/// Handler for the `install-skills` subcommand.
///
/// Resolves the agent list (expanding `All`), iterates over each agent,
/// and calls the appropriate install/uninstall handler.
fn install_skills_command(
    agent: Agent,
    local: bool,
    link: bool,
    uninstall: bool,
    dry_run: bool,
    skills: &[String],
) -> MietteResult<()> {
    let agents = expand_agent_list(agent);
    let mut installed_count = 0u32;

    let project_root = if local { Some(find_project_root()?) } else { None };

    // Resolve all skill sources up front.
    let mut skill_sources: Vec<(String, PathBuf)> = Vec::new();
    if !uninstall {
        for skill in skills {
            let source = resolve_skill_source(skill)?;
            skill_sources.push((skill.clone(), source));
        }
    }

    for ag in &agents {
        let label = agent_label(ag);
        let mode_suffix = if local { " (local)" } else { "" };
        println!("\n{}{}:", label, mode_suffix);

        let result = if uninstall {
            uninstall_agent(ag, local, dry_run, skills, project_root.as_deref())
        } else {
            install_agent(ag, local, link, dry_run, &skill_sources, project_root.as_deref())
        };

        match result {
            Ok(()) => installed_count += 1,
            Err(e) => eprintln!("  error: {e}"),
        }
    }

    let action = if uninstall { "Uninstalled" } else { "Installed" };
    let scope = if local { " locally" } else { "" };
    println!(
        "\n{} rhei skills{} for {} agent{}.",
        action,
        scope,
        installed_count,
        if installed_count == 1 { "" } else { "s" }
    );

    Ok(())
}

/// Expand the `All` agent variant into the full list of concrete agents.
fn expand_agent_list(agent: Agent) -> Vec<Agent> {
    if agent == Agent::All {
        vec![
            Agent::ClaudeCode,
            Agent::Cursor,
            Agent::Windsurf,
            Agent::Copilot,
            Agent::Kilocode,
            Agent::Pi,
            Agent::Codex,
            Agent::Antigravity,
        ]
    } else {
        vec![agent]
    }
}

/// Human-readable label for an agent.
fn agent_label(agent: &Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => "claude-code",
        Agent::Cursor => "cursor",
        Agent::Windsurf => "windsurf",
        Agent::Copilot => "copilot",
        Agent::Kilocode => "kilocode",
        Agent::Pi => "pi",
        Agent::Codex => "codex",
        Agent::Antigravity => "antigravity",
        Agent::All => "all",
    }
}

/// Home directory helper.
fn home_dir() -> MietteResult<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| miette!("HOME environment variable not set"))
}

/// Check whether rhei skills are already installed for a given agent.
fn is_agent_installed(
    agent: &Agent,
    local: bool,
    skills: &[String],
    project_root: Option<&Path>,
) -> bool {
    let check = || -> Option<bool> {
        match agent {
            Agent::ClaudeCode => {
                let base = if local {
                    project_root?.join(".claude")
                } else {
                    home_dir().ok()?.join(".claude")
                };
                // Check for skill directories.
                let first_skill = skills.first()?;
                Some(base.join("skills").join(first_skill).exists())
            }
            Agent::Cursor => {
                let base = if local {
                    project_root?.join(".cursor")
                } else {
                    home_dir().ok()?.join(".cursor")
                };
                let first_skill = skills.first()?;
                Some(base.join("rules").join(format!("{first_skill}.mdc")).exists())
            }
            Agent::Windsurf => {
                let file = if local {
                    project_root?.join(".windsurfrules")
                } else {
                    home_dir().ok()?.join(".windsurfrules")
                };
                Some(has_rhei_markers(&file))
            }
            Agent::Copilot => {
                let file = if local {
                    project_root?.join(".github/copilot-instructions.md")
                } else {
                    home_dir().ok()?.join(".github/copilot-instructions.md")
                };
                Some(has_rhei_markers(&file))
            }
            Agent::Codex => {
                let base = if local {
                    project_root?.join(".codex")
                } else {
                    home_dir().ok()?.join(".codex")
                };
                let first_skill = skills.first()?;
                Some(base.join("instructions").join(format!("{first_skill}.md")).exists())
            }
            Agent::Kilocode | Agent::Pi | Agent::Antigravity => {
                let dir_name = match agent {
                    Agent::Kilocode => ".kilocode",
                    Agent::Pi => ".pi",
                    Agent::Antigravity => ".antigravity",
                    _ => unreachable!(),
                };
                let base = if local {
                    project_root?.join(dir_name)
                } else {
                    home_dir().ok()?.join(dir_name)
                };
                let first_skill = skills.first()?;
                Some(base.join("rules").join(format!("{first_skill}.md")).exists())
            }
            Agent::All => Some(false),
        }
    };
    check().unwrap_or(false)
}

/// Check if a file contains rhei markers.
fn has_rhei_markers(path: &Path) -> bool {
    fs::read_to_string(path).map(|content| content.contains("<!-- rhei:start -->")).unwrap_or(false)
}

/// Install skills for a single agent.
fn install_agent(
    agent: &Agent,
    local: bool,
    link: bool,
    dry_run: bool,
    skill_sources: &[(String, PathBuf)],
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let skill_names: Vec<String> = skill_sources.iter().map(|(n, _)| n.clone()).collect();

    // Check if already installed (skip unless --link forces update).
    if !link && is_agent_installed(agent, local, &skill_names, project_root) {
        println!("  already installed (use --link to force update)");
        return Ok(());
    }

    match agent {
        Agent::ClaudeCode => install_claude_code(skill_sources, local, link, dry_run, project_root),
        Agent::Cursor => install_cursor(skill_sources, local, link, dry_run, project_root),
        Agent::Windsurf => install_windsurf(skill_sources, local, dry_run, project_root),
        Agent::Copilot => install_copilot(skill_sources, local, dry_run, project_root),
        Agent::Kilocode => {
            install_rules_dir_agent(".kilocode", skill_sources, local, link, dry_run, project_root)
        }
        Agent::Pi => {
            install_rules_dir_agent(".pi", skill_sources, local, link, dry_run, project_root)
        }
        Agent::Codex => install_codex(skill_sources, local, link, dry_run, project_root),
        Agent::Antigravity => install_rules_dir_agent(
            ".antigravity",
            skill_sources,
            local,
            link,
            dry_run,
            project_root,
        ),
        Agent::All => Ok(()), // handled by expand_agent_list
    }
}

/// Install skills for Claude Code.
fn install_claude_code(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".claude")
    } else {
        home_dir()?.join(".claude")
    };

    let skills_dir = base.join("skills");

    // Install each skill directory.
    for (name, source) in skill_sources {
        let dest = skills_dir.join(name);
        if link {
            let src =
                if local { relative_path(dest.parent().unwrap(), source) } else { source.clone() };
            link_skill(&src, &dest, dry_run)?;
        } else {
            copy_skill(source, &dest, dry_run)?;
        }
    }

    // Generate and inject registration block into CLAUDE.md.
    let claude_md = base.join("CLAUDE.md");
    let mut block = String::from("# rhei\n");
    for (name, _) in skill_sources {
        let skill_path = if local {
            format!(".claude/skills/{name}/SKILL.md")
        } else {
            format!("~/.claude/skills/{name}/SKILL.md")
        };
        let description = skill_description(name);
        let trigger = format!("/{name}");
        block.push_str(&format!(
            "- **{name}** (`{skill_path}`) — {description}. Trigger: `{trigger}`\n"
        ));
    }
    let trigger_list: Vec<String> =
        skill_sources.iter().map(|(name, _)| format!("`/{name}`")).collect();
    block.push_str(&format!(
        "When the user types {}, invoke the Skill tool with the corresponding skill name before doing anything else.\n",
        trigger_list.join(", ")
    ));

    // Use heading-based injection for Claude Code (not HTML markers).
    inject_claude_md_section(&claude_md, &block, dry_run)?;

    println!("  ✓ {} — registered {} skills", claude_md.display(), skill_sources.len());

    Ok(())
}

/// Inject or replace a `# rhei` section in a CLAUDE.md file.
fn inject_claude_md_section(file: &Path, content: &str, dry_run: bool) -> MietteResult<()> {
    let existing = if file.exists() {
        fs::read_to_string(file).map_err(|e| miette!("failed to read '{}': {e}", file.display()))?
    } else {
        String::new()
    };

    // Check for existing `# rhei` section and replace it.
    let lines: Vec<&str> = existing.lines().collect();
    let mut new_lines: Vec<String> = Vec::new();
    let mut in_rhei_block = false;
    let mut replaced = false;

    for line in &lines {
        if !in_rhei_block {
            if *line == "# rhei" || *line == "## rhei" {
                in_rhei_block = true;
                // Insert new content here.
                for cl in content.lines() {
                    new_lines.push(cl.to_string());
                }
                replaced = true;
                continue;
            }
            new_lines.push(line.to_string());
        } else {
            // Check if we've hit a new heading of equal or higher level.
            let level = line.chars().take_while(|&c| c == '#').count();
            if level > 0 && level <= 2 && !line.starts_with("###") {
                in_rhei_block = false;
                new_lines.push(line.to_string());
            }
            // Skip lines in the old rhei block.
        }
    }

    if !replaced {
        // Append the section.
        if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true) {
            new_lines.push(String::new());
        }
        for cl in content.lines() {
            new_lines.push(cl.to_string());
        }
    }

    let mut final_content = new_lines.join("\n");
    if !final_content.ends_with('\n') {
        final_content.push('\n');
    }

    if dry_run {
        println!("  [dry-run] would update {}", file.display());
        return Ok(());
    }

    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create directory '{}': {e}", parent.display()))?;
    }
    fs::write(file, &final_content)
        .map_err(|e| miette!("failed to write '{}': {e}", file.display()))?;

    Ok(())
}

/// Short description for a skill, used in registration blocks.
fn skill_description(name: &str) -> &'static str {
    match name {
        "rhei-plan-writer" => "create and validate Rhei Plans",
        "rhei-plan-worker" => "execute tasks in a Rhei Plan",
        "rhei-state-machine-writer" => "design custom state machines from project specs and teams",
        _ => "rhei skill",
    }
}

/// Install skills for Cursor (`.mdc` format).
fn install_cursor(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    _link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".cursor")
    } else {
        home_dir()?.join(".cursor")
    };

    let rules_dir = base.join("rules");

    for (name, source) in skill_sources {
        let skill_md = source.join("SKILL.md");
        let content = fs::read_to_string(&skill_md)
            .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;

        let description = skill_description(name);
        let mdc_content = format!(
            "---\ndescription: {description}\nglobs:\n  - \"**/*.rhei.md\"\nalwaysApply: false\n---\n\n{content}"
        );

        let dest = rules_dir.join(format!("{name}.mdc"));

        if dry_run {
            println!("  [dry-run] would write {}", dest.display());
            continue;
        }

        fs::create_dir_all(&rules_dir)
            .map_err(|e| miette!("failed to create '{}': {e}", rules_dir.display()))?;
        fs::write(&dest, &mdc_content)
            .map_err(|e| miette!("failed to write '{}': {e}", dest.display()))?;

        println!("  ✓ {} — written", dest.display());
    }

    Ok(())
}

/// Install skills for agents that use a simple rules directory (Kilocode, Pi, Antigravity).
fn install_rules_dir_agent(
    dir_name: &str,
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(dir_name)
    } else {
        home_dir()?.join(dir_name)
    };

    let rules_dir = base.join("rules");

    for (name, source) in skill_sources {
        let dest = rules_dir.join(format!("{name}.md"));

        if link {
            let skill_md = source.join("SKILL.md");
            let src = if local { relative_path(&rules_dir, &skill_md) } else { skill_md };
            link_skill(&src, &dest, dry_run)?;
        } else {
            let skill_md = source.join("SKILL.md");
            let content = fs::read_to_string(&skill_md)
                .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;

            if dry_run {
                println!("  [dry-run] would write {}", dest.display());
                continue;
            }

            fs::create_dir_all(&rules_dir)
                .map_err(|e| miette!("failed to create '{}': {e}", rules_dir.display()))?;
            fs::write(&dest, &content)
                .map_err(|e| miette!("failed to write '{}': {e}", dest.display()))?;

            println!("  ✓ {} — written", dest.display());
        }
    }

    Ok(())
}

/// Install skills for Windsurf (marker injection).
fn install_windsurf(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let file = if local {
        project_root
            .ok_or_else(|| miette!("--local requires a project root"))?
            .join(".windsurfrules")
    } else {
        // Check alternative global path first.
        let alt = home_dir()?.join(".codeium/windsurf/memories/global_rules.md");
        if alt.exists() {
            alt
        } else {
            home_dir()?.join(".windsurfrules")
        }
    };

    let content = build_marker_content(skill_sources)?;
    inject_marked_section(&file, &content, dry_run)?;

    if !dry_run {
        println!("  ✓ {} — appended rhei section", file.display());
    }

    Ok(())
}

/// Install skills for Copilot (marker injection).
fn install_copilot(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let file = if local {
        project_root
            .ok_or_else(|| miette!("--local requires a project root"))?
            .join(".github/copilot-instructions.md")
    } else {
        home_dir()?.join(".github/copilot-instructions.md")
    };

    let content = build_marker_content(skill_sources)?;
    inject_marked_section(&file, &content, dry_run)?;

    if !dry_run {
        println!("  ✓ {} — appended rhei section", file.display());
    }

    Ok(())
}

/// Install skills for Codex (files + marker injection).
fn install_codex(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".codex")
    } else {
        home_dir()?.join(".codex")
    };

    let instructions_dir = base.join("instructions");

    // Copy/symlink skill files.
    for (name, source) in skill_sources {
        let dest = instructions_dir.join(format!("{name}.md"));

        if link {
            let skill_md = source.join("SKILL.md");
            let src = if local { relative_path(&instructions_dir, &skill_md) } else { skill_md };
            link_skill(&src, &dest, dry_run)?;
        } else {
            let skill_md = source.join("SKILL.md");
            let content = fs::read_to_string(&skill_md)
                .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;

            if dry_run {
                println!("  [dry-run] would write {}", dest.display());
                continue;
            }

            fs::create_dir_all(&instructions_dir)
                .map_err(|e| miette!("failed to create '{}': {e}", instructions_dir.display()))?;
            fs::write(&dest, &content)
                .map_err(|e| miette!("failed to write '{}': {e}", dest.display()))?;

            println!("  ✓ {} — written", dest.display());
        }
    }

    // Inject registration into instructions.md.
    let instructions_md = base.join("instructions.md");
    let content = build_marker_content(skill_sources)?;
    inject_marked_section(&instructions_md, &content, dry_run)?;

    if !dry_run {
        println!("  ✓ {} — appended rhei section", instructions_md.display());
    }

    Ok(())
}

/// Build the content for marker-injected agents (Windsurf, Copilot, Codex).
fn build_marker_content(skill_sources: &[(String, PathBuf)]) -> MietteResult<String> {
    let mut parts = Vec::new();
    for (name, source) in skill_sources {
        let skill_md = source.join("SKILL.md");
        let content = fs::read_to_string(&skill_md)
            .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;
        parts.push(format!(
            "## rhei-{name}\n\nWhen the user asks to create/execute a Rhei plan, follow these instructions:\n\n{content}"
        ));
    }
    Ok(parts.join("\n\n"))
}

/// Uninstall skills for a single agent.
fn uninstall_agent(
    agent: &Agent,
    local: bool,
    dry_run: bool,
    skills: &[String],
    project_root: Option<&Path>,
) -> MietteResult<()> {
    match agent {
        Agent::ClaudeCode => {
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".claude")
            } else {
                home_dir()?.join(".claude")
            };

            // Remove skill directories.
            for skill in skills {
                let dest = base.join("skills").join(skill);
                remove_path(&dest, dry_run)?;
            }

            // Remove registration from CLAUDE.md.
            let claude_md = base.join("CLAUDE.md");
            remove_marked_section(&claude_md, dry_run)?;
        }
        Agent::Cursor => {
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".cursor")
            } else {
                home_dir()?.join(".cursor")
            };

            for skill in skills {
                let dest = base.join("rules").join(format!("{skill}.mdc"));
                remove_path(&dest, dry_run)?;
            }
        }
        Agent::Windsurf => {
            let file = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".windsurfrules")
            } else {
                let alt = home_dir()?.join(".codeium/windsurf/memories/global_rules.md");
                if alt.exists() {
                    alt
                } else {
                    home_dir()?.join(".windsurfrules")
                }
            };
            remove_marked_section(&file, dry_run)?;
        }
        Agent::Copilot => {
            let file = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".github/copilot-instructions.md")
            } else {
                home_dir()?.join(".github/copilot-instructions.md")
            };
            remove_marked_section(&file, dry_run)?;
        }
        Agent::Codex => {
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".codex")
            } else {
                home_dir()?.join(".codex")
            };

            for skill in skills {
                let dest = base.join("instructions").join(format!("{skill}.md"));
                remove_path(&dest, dry_run)?;
            }

            let instructions_md = base.join("instructions.md");
            remove_marked_section(&instructions_md, dry_run)?;
        }
        Agent::Kilocode | Agent::Pi | Agent::Antigravity => {
            let dir_name = match agent {
                Agent::Kilocode => ".kilocode",
                Agent::Pi => ".pi",
                Agent::Antigravity => ".antigravity",
                _ => unreachable!(),
            };
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(dir_name)
            } else {
                home_dir()?.join(dir_name)
            };

            for skill in skills {
                let dest = base.join("rules").join(format!("{skill}.md"));
                remove_path(&dest, dry_run)?;
            }
        }
        Agent::All => {} // handled by expand_agent_list
    }

    println!("  ✓ uninstalled");
    Ok(())
}

/// Remove a file or directory, printing what was done.
fn remove_path(path: &Path, dry_run: bool) -> MietteResult<()> {
    if !path.exists() && path.symlink_metadata().is_err() {
        return Ok(());
    }

    if dry_run {
        println!("  [dry-run] would remove {}", path.display());
        return Ok(());
    }

    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir_all(path)
            .map_err(|e| miette!("failed to remove '{}': {e}", path.display()))?;
    } else {
        fs::remove_file(path).map_err(|e| miette!("failed to remove '{}': {e}", path.display()))?;
    }

    Ok(())
}

/// Recursively copy a skill directory to a destination.
///
/// Creates parent directories as needed. Prints `✓ <dest> — written` on
/// success.
fn copy_skill(src: &Path, dest: &Path, dry_run: bool) -> MietteResult<()> {
    if dry_run {
        println!("  [dry-run] would copy {} → {}", src.display(), dest.display());
        return Ok(());
    }

    if dest.exists() {
        fs::remove_dir_all(dest)
            .map_err(|e| miette!("failed to remove existing '{}': {e}", dest.display()))?;
    }

    copy_dir_recursive(src, dest)?;

    println!("  ✓ {} — written", dest.display());
    Ok(())
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dest: &Path) -> MietteResult<()> {
    fs::create_dir_all(dest)
        .map_err(|e| miette!("failed to create directory '{}': {e}", dest.display()))?;

    for entry in fs::read_dir(src)
        .map_err(|e| miette!("failed to read directory '{}': {e}", src.display()))?
    {
        let entry = entry.map_err(|e| miette!("failed to read dir entry: {e}"))?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| {
                miette!("failed to copy '{}' → '{}': {e}", src_path.display(), dest_path.display())
            })?;
        }
    }

    Ok(())
}

/// Create a symlink from `dest` to `src`.
///
/// For local installs, callers should pass a relative `src` path so the
/// project stays portable. Prints `✓ <dest> → <src>` on success.
fn link_skill(src: &Path, dest: &Path, dry_run: bool) -> MietteResult<()> {
    if dry_run {
        println!("  [dry-run] would symlink {} → {}", dest.display(), src.display());
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create directory '{}': {e}", parent.display()))?;
    }

    // Remove existing symlink or directory.
    if dest.symlink_metadata().is_ok() {
        if dest.is_dir() && !dest.is_symlink() {
            fs::remove_dir_all(dest)
                .map_err(|e| miette!("failed to remove existing '{}': {e}", dest.display()))?;
        } else {
            fs::remove_file(dest)
                .map_err(|e| miette!("failed to remove existing '{}': {e}", dest.display()))?;
        }
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dest).map_err(|e| {
        miette!("failed to symlink '{}' → '{}': {e}", dest.display(), src.display())
    })?;

    #[cfg(not(unix))]
    return Err(miette!("symlinks are only supported on Unix platforms"));

    println!("  ✓ {} → {}", dest.display(), src.display());
    Ok(())
}

/// Compute a relative path from `from_dir` to `to_path`.
///
/// Makes both paths absolute without resolving symlinks (to avoid
/// symlink targets collapsing path differences). Then walks back from
/// `from_dir` and forward to `to_path` via the common ancestor.
fn relative_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    // Make absolute without canonicalizing (no symlink resolution).
    let make_absolute = |p: &Path| -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(p)
        } else {
            p.to_path_buf()
        }
    };

    let from = make_absolute(from_dir);
    let to = make_absolute(to_path);

    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    // Find the common prefix length.
    let common =
        from_components.iter().zip(to_components.iter()).take_while(|(a, b)| a == b).count();

    let mut result = PathBuf::new();
    // Go up from `from_dir` to the common ancestor.
    for _ in common..from_components.len() {
        result.push("..");
    }
    // Go down to `to_path` from the common ancestor.
    for component in &to_components[common..] {
        result.push(component);
    }

    result
}

/// Locate the source directory for a named skill.
///
/// Search order:
/// 1. Installed path: `<binary>/../share/rhei/skills/<skill_name>/`
/// 2. Dev-build fallback: walk up from the binary looking for `Cargo.toml`
///    (the repo root), then check `skills/<skill_name>/`.
fn resolve_skill_source(skill_name: &str) -> MietteResult<PathBuf> {
    // 1. Binary-relative installed path.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            let installed = bin_dir.join("../share/rhei/skills").join(skill_name);
            if installed.is_dir() {
                return installed
                    .canonicalize()
                    .map_err(|e| miette!("failed to canonicalize '{}': {e}", installed.display()));
            }
        }
    }

    // 2. Dev-build fallback: walk up from binary to find repo root (Cargo.toml).
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if d.join("Cargo.toml").is_file() {
                let dev_path = d.join("skills").join(skill_name);
                if dev_path.is_dir() {
                    return dev_path.canonicalize().map_err(|e| {
                        miette!("failed to canonicalize '{}': {e}", dev_path.display())
                    });
                }
                break;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    Err(miette!(
        "could not find skill source directory for '{}'. Searched relative to the rhei binary \
         (../share/rhei/skills/{0}/) and the repo root (skills/{0}/).",
        skill_name
    ))
}

/// Find the project root by walking up from the current directory.
///
/// Looks for common project markers (`.git`, `Cargo.toml`, `package.json`,
/// `pyproject.toml`, `go.mod`). Falls back to the current working directory
/// if no marker is found.
fn find_project_root() -> MietteResult<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette!("failed to determine working directory: {e}"))?;

    let markers = [".git", "Cargo.toml", "package.json", "pyproject.toml", "go.mod"];
    let mut dir = Some(cwd.as_path());
    while let Some(d) = dir {
        for marker in &markers {
            if d.join(marker).exists() {
                return Ok(d.to_path_buf());
            }
        }
        dir = d.parent();
    }

    // Fallback: current working directory.
    Ok(cwd)
}

/// Append or replace a delimited content block in a text file.
///
/// The block is wrapped between `<!-- rhei:start -->` and `<!-- rhei:end -->`
/// markers. If these markers already exist in the file, the content between
/// them is replaced. Otherwise the block is appended. The file is created if
/// it doesn't exist.
fn inject_marked_section(file: &Path, content: &str, dry_run: bool) -> MietteResult<()> {
    let start_marker = "<!-- rhei:start -->";
    let end_marker = "<!-- rhei:end -->";

    let existing = if file.exists() {
        fs::read_to_string(file).map_err(|e| miette!("failed to read '{}': {e}", file.display()))?
    } else {
        String::new()
    };

    let block = format!("{start_marker}\n{content}\n{end_marker}");

    let new_content = if let (Some(start), Some(end)) =
        (existing.find(start_marker), existing.find(end_marker))
    {
        // Replace existing block.
        let before = &existing[..start];
        let after = &existing[end + end_marker.len()..];
        format!("{before}{block}{after}")
    } else {
        // Append.
        if existing.is_empty() {
            block
        } else if existing.ends_with('\n') {
            format!("{existing}\n{block}\n")
        } else {
            format!("{existing}\n\n{block}\n")
        }
    };

    if dry_run {
        println!("  [dry-run] would write {} ({} bytes)", file.display(), new_content.len());
        return Ok(());
    }

    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create directory '{}': {e}", parent.display()))?;
    }
    fs::write(file, &new_content)
        .map_err(|e| miette!("failed to write '{}': {e}", file.display()))?;

    Ok(())
}

/// Remove the `<!-- rhei:start -->` … `<!-- rhei:end -->` block from a file.
///
/// Also handles the `# rhei` heading-based block used by Claude Code's
/// `CLAUDE.md`: removes from `# rhei` (or `## rhei`) to the next heading of
/// equal or higher level, or end of file.
fn remove_marked_section(file: &Path, dry_run: bool) -> MietteResult<()> {
    if !file.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(file)
        .map_err(|e| miette!("failed to read '{}': {e}", file.display()))?;

    let start_marker = "<!-- rhei:start -->";
    let end_marker = "<!-- rhei:end -->";

    let mut result = content.clone();

    // Remove marker-delimited block.
    if let (Some(start), Some(end)) = (result.find(start_marker), result.find(end_marker)) {
        let block_end = end + end_marker.len();
        // Also consume the trailing newline if present.
        let block_end =
            if result[block_end..].starts_with('\n') { block_end + 1 } else { block_end };
        // Also consume a leading blank line before the block.
        let block_start = if start > 0 && result[..start].ends_with('\n') {
            // Check if there's a double newline before the block.
            if start >= 2 && result[..start].ends_with("\n\n") {
                start - 1
            } else {
                start
            }
        } else {
            start
        };
        result = format!("{}{}", &result[..block_start], &result[block_end..]);
    }

    // Remove `# rhei` heading block (Claude Code).
    let lines: Vec<&str> = result.lines().collect();
    let mut new_lines: Vec<&str> = Vec::new();
    let mut in_rhei_block = false;
    let mut rhei_heading_level = 0usize;

    for line in &lines {
        if !in_rhei_block {
            // Detect `# rhei` or `## rhei` heading.
            if (line.starts_with("# rhei") || line.starts_with("## rhei"))
                && !line.starts_with("###")
            {
                in_rhei_block = true;
                rhei_heading_level = line.chars().take_while(|&c| c == '#').count();
                continue;
            }
            new_lines.push(line);
        } else {
            // Check if this line is a heading of equal or higher level.
            let level = line.chars().take_while(|&c| c == '#').count();
            if level > 0 && level <= rhei_heading_level {
                in_rhei_block = false;
                new_lines.push(line);
            }
            // Otherwise skip the line (part of the rhei block).
        }
    }

    let final_content = if new_lines.is_empty() {
        String::new()
    } else {
        let mut s = new_lines.join("\n");
        if content.ends_with('\n') {
            s.push('\n');
        }
        s
    };

    if final_content == content {
        return Ok(());
    }

    if dry_run {
        println!("  [dry-run] would update {}", file.display());
        return Ok(());
    }

    fs::write(file, &final_content)
        .map_err(|e| miette!("failed to write '{}': {e}", file.display()))?;

    Ok(())
}

/// Convert a parser error into an Elm-style diagnostic report.
fn parse_report(path: &Path, input: &str, err: &rhei_core::parser::ParseError) -> Report {
    miette!("{}", render_parse_diagnostic(path, input, err))
}

/// Convert file I/O failures into a consistent diagnostic message.
fn file_io_report(path: &Path, action: &str, err: impl std::fmt::Display) -> Report {
    miette!("{action} '{}': {err}", path.display())
}

/// Convert validation errors into a single CLI-facing diagnostic report.
fn validation_report(input: &Path, state_machine: Option<&Path>, errors: &[String]) -> Report {
    miette!("{}", render_validation_diagnostic(input, state_machine, errors))
}

fn render_parse_diagnostic(
    path: &Path,
    input: &str,
    err: &rhei_core::parser::ParseError,
) -> String {
    let mut lines = vec![format!(
        "-- PARSE ERROR ------------------------------------------------------------- {}",
        path.display()
    )];
    lines.push(String::new());
    lines.push("I got stuck while reading this markdown plan.".to_string());

    if let Some(line_number) = err.line {
        lines.push(String::new());
        lines.push(format!("I was partway through line {line_number} when the problem showed up."));

        if let Some(source_line) = line_text(input, line_number) {
            lines.push(String::new());
            lines.push(format!("{line_number}| {source_line}"));
            lines.push(format!("{}{}", " ".repeat(line_number.to_string().len() + 2), "^"));
        }
    }

    lines.push(String::new());
    lines.push(err.message.replace(" before task content", "\nbefore task content"));
    lines.push(String::new());
    lines.push(
        "Hint: check the markdown structure around the highlighted line and try again.".to_string(),
    );

    lines.join("\n")
}

fn render_validation_diagnostic(
    input: &Path,
    state_machine: Option<&Path>,
    errors: &[String],
) -> String {
    let mut lines = vec![format!(
        "-- VALIDATION ERROR -------------------------------------------------------- {}",
        input.display()
    )];
    lines.push(String::new());
    lines.push(format!(
        "I validated this plan using states from {}, but found a problem.",
        state_machine_label(state_machine),
    ));
    lines.push(String::new());
    lines.push(format_validation_errors(errors));
    lines.push(String::new());
    lines.push("I recommend fixing the problems above and running the command again.".to_string());

    lines.join("\n")
}

fn format_validation_errors(errors: &[String]) -> String {
    if errors.len() == 1 {
        format!("The problem is:\n\n    {}", errors[0])
    } else {
        let mut lines = vec![format!("I found {} problems:", errors.len()), String::new()];
        lines.extend(
            errors.iter().enumerate().map(|(index, error)| format!("{}. {}", index + 1, error)),
        );
        lines.join("\n")
    }
}

fn line_text(input: &str, line_number: usize) -> Option<&str> {
    input.lines().nth(line_number.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn parses_validate_command_with_input() {
        let cli = Cli::try_parse_from(["rhei", "validate", "docs/markdown-plan-compiler.md"])
            .expect("cli should parse");

        assert!(cli.state_machine.is_none());
        match cli.command {
            Commands::Validate { watch, input } => {
                assert!(!watch);
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_validate_watch_command_with_input() {
        let cli =
            Cli::try_parse_from(["rhei", "validate", "--watch", "docs/markdown-plan-compiler.md"])
                .expect("cli should parse");

        assert!(cli.state_machine.is_none());
        match cli.command {
            Commands::Validate { watch, input } => {
                assert!(watch);
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_json_pretty() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "json",
            "--pretty",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { input, format, pretty, no_color, no_metadata, no_content } => {
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
                assert_eq!(format, RenderFormat::Json);
                assert!(pretty);
                assert!(!no_color);
                assert!(!no_metadata);
                assert!(!no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_github_toggles() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "github",
            "--no-metadata",
            "--no-content",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { format, no_metadata, no_content, .. } => {
                assert_eq!(format, RenderFormat::Github);
                assert!(no_metadata);
                assert!(no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_progress_no_color() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "progress",
            "--no-color",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { format, no_color, .. } => {
                assert_eq!(format, RenderFormat::Progress);
                assert!(no_color);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_states_command() {
        let cli = Cli::try_parse_from(["rhei", "states"]).expect("cli should parse");
        match cli.command {
            Commands::States { json } => assert!(!json),
            other => panic!("expected states command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["rhei", "states", "--json"]).expect("cli should parse");
        match cli.command {
            Commands::States { json } => assert!(json),
            other => panic!("expected states command, got {other:?}"),
        }
    }

    #[test]
    fn render_state_machine_text_includes_states_and_transitions() {
        let yaml = r#"
name: demo
personality: You are an MIT professor.
version: 1
models:
  - gpt-5
  - claude-sonnet
states:
  draft:
    description: planning
    instructions: Wait until author promotes task.
    initial: true
    iterations: 3
    all_models:
      - gpt-5
      - claude-sonnet
  done:
    description: finished
    model: gpt-5
    final: true
transitions:
  - from: draft
    to: done
    on_enter: cli:record_done
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_text(&machine);

        assert!(rendered.contains("State machine: demo"));
        assert!(rendered.contains("Personality: You are an MIT professor."));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("draft [initial]"));
        assert!(rendered.contains("Iterations: 3"));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("Wait until author promotes task."));
        assert!(rendered.contains("done [final]"));
        assert!(rendered.contains("Model: gpt-5"));
        assert!(rendered.contains("draft -> done (on_enter=cli:record_done)"));
    }

    #[test]
    fn render_state_machine_json_includes_personality() {
        let yaml = r#"
name: demo
personality: You are an MIT professor.
version: 1
models:
  - gpt-5
states:
  draft:
    description: planning
    iterations: 2
    all_models:
      - gpt-5
    initial: true
transitions: []
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_json(&machine).expect("render JSON");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("parse JSON");

        assert_eq!(json["name"], "demo");
        assert_eq!(json["personality"], "You are an MIT professor.");
        assert_eq!(json["models"], serde_json::json!(["gpt-5"]));
        assert_eq!(json["states"][0]["iterations"], 2);
        assert_eq!(json["states"][0]["all_models"], serde_json::json!(["gpt-5"]));
    }

    #[test]
    fn parses_version_command() {
        let cli = Cli::try_parse_from(["rhei", "version"]).expect("cli should parse");

        match cli.command {
            Commands::Version => {}
            other => panic!("expected version command, got {other:?}"),
        }
    }

    #[test]
    fn render_rhei_json_smoke() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Smoke

## Tasks

### Task 1: Alpha
**State:** pending
"#,
        )
        .expect("parse should succeed");

        let rendered =
            render_rhei(&rhei, RenderFormat::Json, true, false, false, false).expect("render ok");

        assert!(rendered.contains("\"title\": \"Smoke\""));
        assert!(rendered.contains("\"tasks\""));
    }

    #[test]
    fn parse_diagnostic_includes_line_info_when_available() {
        let input = "first line\nbad line\nthird line";
        let err = rhei_core::parser::ParseError {
            message: "unexpected token".to_string(),
            line: Some(2),
        };

        let rendered = render_parse_diagnostic(Path::new("broken.md"), input, &err);

        assert!(rendered.contains("-- PARSE ERROR"));
        assert!(rendered.contains("broken.md"));
        assert!(rendered.contains("2| bad line"));
        assert!(rendered.contains("unexpected token"));
    }

    #[test]
    fn validation_failure_formatting_aggregates_multiple_errors() {
        let rendered = format_validation_errors(&[
            "Task 1 is missing mandatory **State:** metadata".to_string(),
            "Task 2 depends on missing Task 9".to_string(),
        ]);

        assert!(rendered.contains("I found 2 problems:"));
        assert!(rendered.contains("1. Task 1 is missing mandatory **State:** metadata"));
        assert!(rendered.contains("2. Task 2 depends on missing Task 9"));
    }

    #[test]
    fn path_matches_normalizes_paths() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/states.yaml"),
        );

        assert!(path_matches(Path::new("./docs/markdown-plan-compiler.md"), &watched));
        assert!(path_matches(Path::new("docs/states.yaml"), &watched));
        assert!(!path_matches(Path::new("docs/plan-language-spec.md"), &watched));
    }

    #[test]
    fn paths_equivalent_falls_back_for_nonexistent_relative_paths() {
        assert!(paths_equivalent(
            Path::new("./docs/markdown-plan-compiler.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
        assert!(!paths_equivalent(
            Path::new("./docs/plan-language-spec.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
    }

    #[test]
    fn should_revalidate_filters_irrelevant_events() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/states.yaml"),
        );

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(should_revalidate(&event, &watched));

        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(!should_revalidate(&event, &watched));
    }

    #[test]
    fn parses_complete_command_with_result() {
        let cli = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "3",
            "--result",
            "All tests pass",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Complete { input, task, result, no_callbacks } => {
                assert_eq!(input, PathBuf::from("plan.rhei.md"));
                assert_eq!(task, "3");
                assert_eq!(result, "All tests pass");
                assert!(!no_callbacks);
            }
            other => panic!("expected complete command, got {other:?}"),
        }
    }

    #[test]
    fn parses_complete_command_requires_result() {
        // --result is mandatory; omitting it should fail.
        let err = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "build",
            "--no-callbacks",
        ]);
        assert!(err.is_err(), "complete without --result should fail");
    }

    #[test]
    fn parses_reset_command() {
        let cli = Cli::try_parse_from(["rhei", "reset", "workspace"]).expect("cli should parse");

        match cli.command {
            Commands::Reset { input } => {
                assert_eq!(input, PathBuf::from("workspace"));
            }
            other => panic!("expected reset command, got {other:?}"),
        }
    }

    #[test]
    fn find_completion_state_prefers_non_cancelled_terminal() {
        let yaml = r#"
name: test
version: 1
states:
  active: { description: "working" }
  completed: { description: "done", final: true }
  cancelled: { description: "nope", final: true }
transitions:
  - from: active
    to: cancelled
  - from: active
    to: completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let target = find_completion_state("active", &machine);
        assert_eq!(target.as_deref(), Some("completed"));
    }

    #[test]
    fn find_completion_state_does_not_fall_back_to_cancelled() {
        let yaml = r#"
name: test
version: 1
states:
  active: { description: "working" }
  cancelled: { description: "nope", final: true }
transitions:
  - from: active
    to: cancelled
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let target = find_completion_state("active", &machine);
        assert!(target.is_none(), "complete should not treat cancellation as success");
    }

    #[test]
    fn find_completion_state_returns_none_when_no_terminal_reachable() {
        let yaml = r#"
name: test
version: 1
states:
  draft: { description: "initial", initial: true }
  pending: { description: "ready" }
  completed: { description: "done", final: true }
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        // draft can only go to pending (non-terminal), not directly to completed
        let target = find_completion_state("draft", &machine);
        assert!(target.is_none());
    }

    #[test]
    fn rewrite_task_completion_removes_assignee_and_appends_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"# Rhei: Test

## Tasks

### Task 1: Alpha
**State:** completed
**Assignee:** agent-1
Some work description.
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", "1", "runtime/results/1.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"), "assignee should be removed");
        assert!(
            content.contains("> **Result:** [1](runtime/results/1.md)"),
            "result link should be appended"
        );
        // State line should remain
        assert!(content.contains("**State:** completed"));
    }

    #[test]
    fn rewrite_task_completion_without_assignee_still_appends_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 1: Alpha
**State:** completed
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", "1", "runtime/results/1.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("> **Result:** [1](runtime/results/1.md)"));
    }

    #[test]
    fn rewrite_all_states_to_initial_updates_tasks_and_subtasks() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Alpha
**State:** completed

#### Subtask 1.1: Detail
**State:** in-progress

### Task 2: Beta
**State:** review
"#;

        let rewritten = rewrite_all_states_to_initial(raw, "pending").expect("rewrite states");

        assert_eq!(rewritten.matches("**State:** pending").count(), 3);
        assert!(!rewritten.contains("**State:** completed"));
        assert!(!rewritten.contains("**State:** in-progress"));
        assert!(!rewritten.contains("**State:** review"));
    }

    #[test]
    fn rewrite_task_completion_inserts_result_link_before_subtask() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 2: Beta
**State:** completed

Body text.

#### Subtask 2.1: Sub
**State:** completed
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "2", "2", "runtime/results/2.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        let result_pos =
            content.find("> **Result:** [2](runtime/results/2.md)").expect("result present");
        let subtask_pos = content.find("#### Subtask 2.1").expect("subtask present");
        assert!(result_pos < subtask_pos, "result should appear before subtask");
    }

    #[test]
    fn clap_command_factory_builds() {
        Cli::command().debug_assert();
    }
}
