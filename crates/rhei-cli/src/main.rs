use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use fs2::FileExt;
use miette::{miette, Report, Result as MietteResult};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rhei_core::ast::TaskId;
use rhei_core::callback::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
use rhei_core::workspace;
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
  complete    Mark a task as completed: transition to a terminal state, remove the assignee,\n              and optionally log a result message

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
    /// Mark a task as completed: transition to a terminal state, remove the
    /// assignee, and optionally log a result message in the task body.
    Complete {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN")]
        input: PathBuf,
        /// Task identifier (number or name)
        #[arg(long)]
        task: String,
        /// Result message to record in the task body
        #[arg(long)]
        result: Option<String>,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Print versions for the CLI and related crates
    Version,
}

/// Output formats supported by the [`Render`](Commands::Render) subcommand.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum RenderFormat {
    Json,
    Github,
    Progress,
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
        Commands::Complete { input, task, result, no_callbacks } => complete_command(
            &input,
            cli.state_machine.as_deref(),
            &task,
            result.as_deref(),
            no_callbacks,
        ),
        Commands::Version => {
            print_versions();
            Ok(())
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
                "initial": def.initial,
                "final": def.terminal,
            })
        })
        .collect();

    let transitions =
        serde_json::to_value(&machine.transitions).context("serialize transitions")?;
    let version =
        serde_json::to_value(&machine.version).context("serialize state machine version")?;

    let payload = serde_json::json!({
        "name": machine.name,
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

    let task_file = if workspace::is_workspace(input) {
        let loaded = load_plan(input)?;
        loaded.task_file(task_id_str, input)
    } else {
        input.to_path_buf()
    };

    execute_transition(&task_file, input, &machine, task_id_str, from, to, no_callbacks)?;
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
    task_file: &Path,
    plan_path: &Path,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    // Validate that both `from` and `to` are valid states.
    if !machine.is_valid_state(from) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!("'{}' is not a valid state. Allowed: [{}]", from, allowed));
    }
    if !machine.is_valid_state(to) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!("'{}' is not a valid state. Allowed: [{}]", to, allowed));
    }

    // Validate that the from→to transition is allowed by the state machine.
    let transition_allowed = machine
        .transitions()
        .iter()
        .any(|rule| (rule.from.0 == from || rule.from.0 == "*") && rule.to.0 == to);
    if !transition_allowed {
        return Err(miette!(
            "transition from '{}' to '{}' is not allowed by the state machine",
            from,
            to
        ));
    }

    // Open the file with an exclusive lock for the duration of the operation.
    let file = fs::File::open(task_file)
        .map_err(|err| file_io_report(task_file, "failed to open plan file", err))?;
    file.lock_exclusive()
        .map_err(|err| file_io_report(task_file, "failed to acquire file lock", err))?;

    // Read the raw markdown while holding the lock.
    let raw = fs::read_to_string(task_file)
        .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?;

    // Parse to validate structure and find the task.
    // Try full plan parse first; fall back to workspace task-file parse.
    let target_id = parse_task_id(task_id_str);
    let current_state = find_task_current_state(&raw, task_file, &target_id, task_id_str)?;

    // Compare-and-swap: verify the task's current state matches `from`.
    if current_state != from {
        let _ = file.unlock();
        return Err(miette!(
            "conflict: Task {} is in state '{}', expected '{}'",
            task_id_str,
            current_state,
            from
        ));
    }

    // Find the matching transition rule for callback lookup.
    let matching_rule = machine
        .transitions()
        .iter()
        .find(|rule| (rule.from.0 == from || rule.from.0 == "*") && rule.to.0 == to);

    // Execute on_leave callback before the state change.
    let callback_ctx =
        CallbackContext { task_id: task_id_str, from_state: from, to_state: to, plan_path };

    if !no_callbacks {
        if let Some(rule) = matching_rule {
            if let Some(ref cb) = rule.on_leave {
                let executor = ShellCallbackExecutor;
                let result = executor.execute(cb, &callback_ctx).map_err(|e| miette!("{e}"))?;
                if !result.success {
                    let _ = file.unlock();
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

    // Perform text-level replacement of the **State:** line for this task.
    let new_raw = rewrite_task_state(&raw, task_id_str, to)?;

    // Atomic write: write to a temp file in the same directory, then rename.
    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(task_file).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    // Execute on_enter callback after the state change.
    if !no_callbacks {
        if let Some(rule) = matching_rule {
            if let Some(ref cb) = rule.on_enter {
                let executor = ShellCallbackExecutor;
                let result = executor.execute(cb, &callback_ctx).map_err(|e| miette!("{e}"))?;
                if !result.success {
                    let stderr = result.stderr.trim();
                    let detail =
                        if stderr.is_empty() { String::new() } else { format!(": {stderr}") };
                    eprintln!(
                        "warning: on_enter callback '{}' failed after state change{detail}",
                        cb.0
                    );
                }
            }
        }
    }

    let _ = file.unlock();
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

        for (task_id_str, current_state) in &ready {
            // Find the next forward transition (explicit from-state match, not wildcard).
            let next_to = find_next_transition(current_state, &machine);

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

            let task_file = loaded.task_file(task_id_str, input);
            match execute_transition(
                &task_file,
                input,
                &machine,
                task_id_str,
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

/// Find tasks that are ready to advance: not in a terminal state and all
/// prior dependencies are in terminal states.
///
/// Returns a vec of `(task_id_string, current_state)` pairs.
fn find_ready_tasks(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> Vec<(String, String)> {
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

        // Check that all prior dependencies are in terminal states.
        let all_priors_done = task.prior.iter().all(|dep_id| {
            state_map.get(dep_id).map(|s| is_terminal_state(s, machine)).unwrap_or(false)
        });

        if all_priors_done {
            ready.push((task.id.to_string(), current_state.to_string()));
        }
    }

    ready
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
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<String> {
    // First, look for an exact from-state match.
    for rule in machine.transitions() {
        if rule.from.0 == current_state {
            return Some(rule.to.0.clone());
        }
    }

    // Fall back to wildcard, but only to non-terminal states (forward progress).
    for rule in machine.transitions() {
        if rule.from.0 == "*" {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if !is_terminal {
                return Some(rule.to.0.clone());
            }
        }
    }

    None
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
        let state = task.state.as_str().to_string();
        (tid.to_string(), state)
    } else {
        let ready = find_ready_tasks(&loaded.rhei, &machine);
        if ready.is_empty() {
            return Err(miette!("no tasks are ready to advance"));
        }
        ready.into_iter().next().unwrap()
    };

    // Determine whether we need a state transition.
    // Tasks in an initial state (e.g. draft) are transitioned forward.
    let is_initial = machine.states.get(&current_state).map(|d| d.initial).unwrap_or(false);

    let task_file = loaded.task_file(&task_id_str, input);

    let final_state = if is_initial {
        // Advance from the initial state (e.g. draft → pending).
        let to_state = find_next_transition(&current_state, &machine).ok_or_else(|| {
            miette!("no forward transition available from state '{}'", current_state)
        })?;
        execute_transition(
            &task_file,
            input,
            &machine,
            &task_id_str,
            &current_state,
            &to_state,
            no_callbacks,
        )?;
        to_state
    } else {
        current_state.clone()
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
    print_next_output(as_json, task, &current_state, &final_state, &instructions);

    Ok(())
}

/// Execute the `complete` subcommand: transition a task to a terminal state,
/// remove its assignee, and optionally append a result message.
///
/// The target terminal state is chosen automatically: the first non-cancelled
/// terminal state reachable from the task's current state via a declared
/// transition. If no such transition exists, the command fails.
fn complete_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    result_msg: Option<&str>,
    no_callbacks: bool,
) -> MietteResult<()> {
    let machine = load_state_machine(state_machine_path)?;

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
    execute_transition(
        &task_file,
        input,
        &machine,
        task_id_str,
        current_state,
        &to_state,
        no_callbacks,
    )?;

    // Post-transition: remove assignee and append result.
    rewrite_task_completion(&task_file, task_id_str, result_msg)?;

    println!("Task {} completed: '{}' → '{}'", task_id_str, current_state, to_state);
    if let Some(msg) = result_msg {
        println!("Result: {}", msg);
    }

    Ok(())
}

/// Find a terminal (non-cancelled) state reachable in one transition.
///
/// Prefers exact `from` matches over wildcards, and prefers non-cancelled
/// terminal states over `cancelled`.
fn find_completion_state(
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<String> {
    let mut cancelled_fallback: Option<String> = None;

    // Exact from-state matches first.
    for rule in machine.transitions() {
        if rule.from.0 == current_state {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal {
                if rule.to.0 == "cancelled" {
                    cancelled_fallback.get_or_insert_with(|| rule.to.0.clone());
                } else {
                    return Some(rule.to.0.clone());
                }
            }
        }
    }

    // Fall back to wildcard transitions.
    for rule in machine.transitions() {
        if rule.from.0 == "*" {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal {
                if rule.to.0 == "cancelled" {
                    cancelled_fallback.get_or_insert_with(|| rule.to.0.clone());
                } else {
                    return Some(rule.to.0.clone());
                }
            }
        }
    }

    cancelled_fallback
}

/// Rewrite a task's markdown after completion: remove `**Assignee:**` and
/// optionally append a `> **Result:** <msg>` block to the task body.
///
/// Operates on raw text lines so the parser does not need to know about
/// assignee or result fields.
fn rewrite_task_completion(
    task_file: &Path,
    task_id: &str,
    result_msg: Option<&str>,
) -> MietteResult<()> {
    let raw = fs::read_to_string(task_file)
        .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?;

    let lines: Vec<&str> = raw.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let task_prefix = format!("### Task {}:", task_id);

    let mut in_target_task = false;
    let mut result_inserted = false;

    for line in &lines {
        let is_new_task = line.starts_with("### Task ") && !line.starts_with(&task_prefix);
        let is_subtask = line.starts_with("#### Subtask ");

        // When we hit a new structural element while still inside the target
        // task, insert the result block before that element.
        if in_target_task && !result_inserted && (is_new_task || is_subtask) {
            if let Some(msg) = result_msg {
                result_lines.push(String::new());
                result_lines.push(format!("> **Result:** {}", msg));
            }
            result_inserted = true;
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
    if in_target_task && !result_inserted {
        if let Some(msg) = result_msg {
            result_lines.push(String::new());
            result_lines.push(format!("> **Result:** {}", msg));
        }
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
version: 1
states:
  draft:
    description: planning
    instructions: Wait until author promotes task.
    initial: true
  done:
    description: finished
    final: true
transitions:
  - from: draft
    to: done
    on_enter: cli:record_done
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_text(&machine);

        assert!(rendered.contains("State machine: demo"));
        assert!(rendered.contains("draft [initial]"));
        assert!(rendered.contains("Wait until author promotes task."));
        assert!(rendered.contains("done [final]"));
        assert!(rendered.contains("draft -> done (on_enter=cli:record_done)"));
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
                assert_eq!(result.as_deref(), Some("All tests pass"));
                assert!(!no_callbacks);
            }
            other => panic!("expected complete command, got {other:?}"),
        }
    }

    #[test]
    fn parses_complete_command_without_result() {
        let cli = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "build",
            "--no-callbacks",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Complete { input, task, result, no_callbacks } => {
                assert_eq!(input, PathBuf::from("plan.rhei.md"));
                assert_eq!(task, "build");
                assert!(result.is_none());
                assert!(no_callbacks);
            }
            other => panic!("expected complete command, got {other:?}"),
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
    fn find_completion_state_falls_back_to_cancelled() {
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
        assert_eq!(target.as_deref(), Some("cancelled"));
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
    fn rewrite_task_completion_removes_assignee_and_appends_result() {
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

        rewrite_task_completion(&path, "1", Some("All tests pass")).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"), "assignee should be removed");
        assert!(content.contains("> **Result:** All tests pass"), "result should be appended");
        // State line should remain
        assert!(content.contains("**State:** completed"));
    }

    #[test]
    fn rewrite_task_completion_without_result_only_removes_assignee() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 1: Alpha
**State:** completed
**Assignee:** bot
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", None).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"));
        assert!(!content.contains("**Result:**"));
    }

    #[test]
    fn rewrite_task_completion_inserts_result_before_subtask() {
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

        rewrite_task_completion(&path, "2", Some("Done")).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        let result_pos = content.find("> **Result:** Done").expect("result present");
        let subtask_pos = content.find("#### Subtask 2.1").expect("subtask present");
        assert!(result_pos < subtask_pos, "result should appear before subtask");
    }

    #[test]
    fn clap_command_factory_builds() {
        Cli::command().debug_assert();
    }
}
