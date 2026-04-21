use anyhow::{Context, Result};
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use fs2::FileExt;
use miette::{miette, Report, Result as MietteResult};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use rhei_core::ast::{Metadata, TaskId};
use rhei_core::callback::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
use rhei_core::workspace;
use rhei_validator::{
    AgentConfig, CustomAgentProfile, McpServerProfile, SkillProfile, StateMcpEntry,
    StateMcpEntryObject, StateSkillEntry,
};
use serde::Deserialize;
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

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

Templates:
  templates   List available templates
  instantiate Instantiate a template into a concrete plan or workspace

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
        #[command(flatten)]
        standalone: StandaloneExecutionFlags,
        #[command(flatten)]
        agent: AgentExecutionFlags,
        #[command(flatten)]
        program: ProgramExecutionFlags,
    },
    /// List available templates
    Templates {
        /// Emit the template list as JSON instead of plain text
        #[arg(long)]
        json: bool,
        /// Filter by discovery source: project, user, or all
        #[arg(long, default_value = "all", value_name = "SOURCE")]
        source: String,
    },
    /// Instantiate a template into a concrete plan or workspace
    Instantiate {
        /// Template name or path to a template directory
        #[arg(value_name = "TEMPLATE")]
        template: String,
        /// Set an input value (repeatable)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set_values: Vec<String>,
        /// Set an input value from file contents (repeatable)
        #[arg(long = "set-file", value_name = "KEY=PATH")]
        set_files: Vec<String>,
        /// Load input values from a YAML or JSON file (repeatable)
        #[arg(long, value_name = "FILE")]
        values: Vec<PathBuf>,
        /// Output directory
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Instantiate and immediately begin execution
        #[arg(long)]
        execute: bool,
        /// Show what would be generated without writing files
        #[arg(long)]
        dry_run: bool,
        /// Keep the output directory on validation failure
        #[arg(long)]
        keep_on_error: bool,
        /// Print the template input schema and exit
        #[arg(long)]
        list_inputs: bool,
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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            let mut cmd = Cli::command();
            cmd.print_help().map_err(|io_err| miette!("failed to write CLI help: {io_err}"))?;
            println!();
            return Ok(());
        }
        Err(err) => err.exit(),
    };

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
        Commands::Run { input, standalone, agent, program } => {
            run_command(&input, cli.state_machine.as_deref(), (standalone, agent, program).into())
        }
        Commands::Templates { json, source } => {
            template_impl_unused::templates_command(json, &source)
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
        } => template_impl_unused::instantiate_command(
            &template,
            &set_values,
            &set_files,
            &values,
            output.as_deref(),
            execute,
            dry_run,
            keep_on_error,
            list_inputs,
        ),
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

struct ResolvedStateMachine {
    machine: rhei_validator::StateMachine,
    path: Option<PathBuf>,
}

fn auto_state_machine_path(input: &Path) -> PathBuf {
    if workspace::is_workspace(input) {
        input.join("states.yaml")
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml")
    }
}

fn resolve_state_machine_for_loaded_plan(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
) -> MietteResult<ResolvedStateMachine> {
    if let Some(path) = state_machine_path {
        return Ok(ResolvedStateMachine {
            machine: load_state_machine(Some(path))?,
            path: Some(path.to_path_buf()),
        });
    }

    let builtin = rhei_validator::StateMachine::builtin_default();
    let declared_name = loaded.rhei.states.trim();
    let candidate = auto_state_machine_path(input);

    if candidate.is_file() {
        let machine = load_state_machine(Some(&candidate))?;
        if machine.name == declared_name {
            return Ok(ResolvedStateMachine { machine, path: Some(candidate) });
        }

        if declared_name != builtin.name {
            return Err(miette!(
                "plan declares state machine '{}', but auto-discovered states file '{}' declares '{}'",
                declared_name,
                candidate.display(),
                machine.name
            ));
        }
    }

    if declared_name != builtin.name {
        return Err(miette!(
            "plan declares state machine '{}', but no auto-discovered states file was found at '{}'.\nUse --state-machine <path> to override the default location.",
            declared_name,
            candidate.display()
        ));
    }

    Ok(ResolvedStateMachine { machine: builtin, path: None })
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

#[allow(dead_code, clippy::all)]
mod template_impl_unused {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateSource {
        Project,
        User,
    }

    impl TemplateSource {
        fn as_str(self) -> &'static str {
            match self {
                TemplateSource::Project => "project",
                TemplateSource::User => "user",
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateSourceFilter {
        Project,
        User,
        All,
    }

    impl TemplateSourceFilter {
        fn includes(self, source: TemplateSource) -> bool {
            matches!(
                (self, source),
                (TemplateSourceFilter::All, _)
                    | (TemplateSourceFilter::Project, TemplateSource::Project)
                    | (TemplateSourceFilter::User, TemplateSource::User)
            )
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateLayout {
        SingleFile,
        Workspace,
    }

    impl TemplateLayout {
        fn entrypoint(self, output_dir: &Path) -> PathBuf {
            match self {
                TemplateLayout::SingleFile => output_dir.join("plan.rhei.md"),
                TemplateLayout::Workspace => output_dir.to_path_buf(),
            }
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TemplateManifest {
        name: String,
        version: YamlValue,
        description: String,
        #[serde(default)]
        inputs: Vec<TemplateInputDef>,
    }

    impl TemplateManifest {
        fn version_string(&self) -> String {
            format_version(&self.version)
        }

        fn required_input_count(&self) -> usize {
            self.inputs.iter().filter(|input| input.is_required()).count()
        }

        fn inputs_summary(&self) -> String {
            if self.inputs.is_empty() {
                return "none".to_string();
            }

            self.inputs
                .iter()
                .map(|input| {
                    if input.is_required() {
                        input.name.clone()
                    } else {
                        format!("{}?", input.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TemplateInputDef {
        name: String,
        description: String,
        #[serde(default, rename = "type")]
        value_type: TemplateInputType,
        #[serde(default)]
        required: Option<bool>,
        #[serde(default)]
        default: Option<YamlValue>,
        #[serde(default)]
        validate: Option<String>,
    }

    impl TemplateInputDef {
        fn is_required(&self) -> bool {
            self.required.unwrap_or(self.default.is_none())
        }
    }

    #[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum TemplateInputType {
        #[default]
        String,
        Number,
        Boolean,
        Path,
    }

    impl TemplateInputType {
        fn as_str(self) -> &'static str {
            match self {
                TemplateInputType::String => "string",
                TemplateInputType::Number => "number",
                TemplateInputType::Boolean => "boolean",
                TemplateInputType::Path => "path",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct DiscoveredTemplate {
        manifest: TemplateManifest,
        path: PathBuf,
        source: TemplateSource,
    }

    #[derive(Debug)]
    struct MaterializedTemplate {
        layout: TemplateLayout,
        output_dir: PathBuf,
        generated_files: Vec<PathBuf>,
    }

    impl MaterializedTemplate {
        fn entrypoint(&self) -> PathBuf {
            self.layout.entrypoint(&self.output_dir)
        }

        fn state_machine_path(&self) -> Option<PathBuf> {
            let path = self.output_dir.join("states.yaml");
            path.is_file().then_some(path)
        }
    }

    pub(super) fn templates_command(as_json: bool, source_filter: &str) -> MietteResult<()> {
        let filter = parse_template_source_filter(source_filter)?;
        let templates = discover_templates(filter)?;

        if as_json {
            let payload = templates
                .iter()
                .map(|template| {
                    serde_json::json!({
                        "name": template.manifest.name,
                        "version": template.manifest.version_string(),
                        "description": template.manifest.description,
                        "source": template.source.as_str(),
                        "path": template.path,
                        "required_inputs": template.manifest.required_input_count(),
                        "inputs": template.manifest.inputs.iter().map(|input| {
                            serde_json::json!({
                                "name": input.name,
                                "type": input.value_type.as_str(),
                                "required": input.is_required(),
                                "description": input.description,
                                "default": input.default,
                                "validate": input.validate,
                            })
                        }).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            let rendered = serde_json::to_string_pretty(&payload)
                .map_err(|err| miette!("failed to serialize template listing: {err}"))?;
            println!("{rendered}");
            return Ok(());
        }

        if templates.is_empty() {
            println!("No templates found.");
            return Ok(());
        }

        println!("Templates:");
        for template in templates {
            println!(
                "{}  {}  {}",
                template.manifest.name,
                template.manifest.version_string(),
                template.source.as_str(),
            );
            println!("  {}", template.manifest.description);
            println!("  inputs: {}", template.manifest.inputs_summary());
        }

        Ok(())
    }

    pub(super) fn instantiate_command(
        template: &str,
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output: Option<&Path>,
        execute: bool,
        dry_run: bool,
        keep_on_error: bool,
        list_inputs: bool,
    ) -> MietteResult<()> {
        if execute && dry_run {
            return Err(miette!("--execute cannot be used together with --dry-run"));
        }

        let template_dir = resolve_template_reference(template)?;
        let manifest = load_template_manifest(&template_dir)?;

        if list_inputs {
            print_template_inputs(&manifest);
            return Ok(());
        }

        let layout = detect_template_layout(&template_dir)?;
        let resolved_values =
            collect_template_inputs(&manifest, values_files, set_values, set_files)?;
        let default_output = std::env::current_dir()
            .map_err(|err| miette!("failed to determine working directory: {err}"))?
            .join(template_dir.file_name().ok_or_else(|| {
                miette!("template path '{}' has no directory name", template_dir.display())
            })?);
        let output_dir = output.map(Path::to_path_buf).unwrap_or(default_output);

        if !dry_run && output_dir.exists() {
            return Err(miette!("output path '{}' already exists", output_dir.display()));
        }

        let scratch = if dry_run {
            Some(
                tempfile::tempdir()
                    .map_err(|err| miette!("failed to create temporary output directory: {err}"))?,
            )
        } else {
            None
        };
        let target_dir = scratch
            .as_ref()
            .map(|dir| dir.path().join("instantiate-output"))
            .unwrap_or_else(|| output_dir.clone());

        let materialized =
            match materialize_template(&template_dir, layout, &target_dir, &resolved_values) {
                Ok(materialized) => materialized,
                Err(err) => {
                    if !dry_run {
                        let _ = remove_path(&target_dir, false);
                    }
                    return Err(err);
                }
            };

        let entrypoint = materialized.entrypoint();
        let state_machine_path = materialized.state_machine_path();

        if let Err(err) = run_validation_once(&entrypoint, state_machine_path.as_deref()) {
            if !dry_run && !keep_on_error {
                let _ = remove_path(&target_dir, false);
            }
            return Err(err);
        }

        if dry_run {
            println!(
                "Dry run OK: '{}' would be instantiated into '{}'.",
                manifest.name,
                output_dir.display()
            );
            for path in &materialized.generated_files {
                println!("  {}", path.display());
            }
            return Ok(());
        }

        println!("Instantiated template '{}' into '{}'.", manifest.name, output_dir.display());

        if execute {
            let opts = RunOptions {
                standalone: StandaloneExecutionFlags::default(),
                agent: AgentExecutionFlags::default(),
                program: ProgramExecutionFlags::default(),
            };
            return run_command(&entrypoint, state_machine_path.as_deref(), opts);
        }

        Ok(())
    }

    fn parse_template_source_filter(value: &str) -> MietteResult<TemplateSourceFilter> {
        match value.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(TemplateSourceFilter::Project),
            "user" => Ok(TemplateSourceFilter::User),
            "all" => Ok(TemplateSourceFilter::All),
            other => Err(miette!(
                "invalid template source '{}'. Expected one of: project, user, all",
                other
            )),
        }
    }

    fn discover_templates(filter: TemplateSourceFilter) -> MietteResult<Vec<DiscoveredTemplate>> {
        let mut templates = Vec::new();
        let mut seen = HashSet::new();

        for (source, root) in template_search_roots(filter)? {
            if !root.is_dir() {
                continue;
            }

            let mut entries = fs::read_dir(&root)
                .map_err(|err| file_io_report(&root, "failed to read template directory", err))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    miette!("failed to read dir entry in '{}': {err}", root.display())
                })?;
            entries.sort_by_key(|entry| entry.file_name());

            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || !path.is_dir() || seen.contains(&name) {
                    continue;
                }

                let Ok(manifest) = load_template_manifest(&path) else {
                    continue;
                };

                seen.insert(name);
                templates.push(DiscoveredTemplate { manifest, path, source });
            }
        }

        Ok(templates)
    }

    fn template_search_roots(
        filter: TemplateSourceFilter,
    ) -> MietteResult<Vec<(TemplateSource, PathBuf)>> {
        let mut roots = Vec::new();

        if filter.includes(TemplateSource::Project) {
            roots.push((
                TemplateSource::Project,
                find_project_root()?.join(".agents").join("rhei").join("templates"),
            ));
        }
        if filter.includes(TemplateSource::User) {
            roots.push((
                TemplateSource::User,
                home_dir()?.join(".agents").join("rhei").join("templates"),
            ));
        }

        Ok(roots)
    }

    fn resolve_template_reference(reference: &str) -> MietteResult<PathBuf> {
        if template_reference_is_path(reference) {
            let path = PathBuf::from(reference);
            if !path.is_dir() {
                return Err(miette!("template directory '{}' does not exist", path.display()));
            }
            return Ok(path);
        }

        for (_, root) in template_search_roots(TemplateSourceFilter::All)? {
            let candidate = root.join(reference);
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }

        Err(miette!("template '{}' not found in project or user template directories", reference))
    }

    fn template_reference_is_path(reference: &str) -> bool {
        let path = Path::new(reference);
        path.is_absolute() || reference.contains('/') || reference.starts_with('.')
    }

    fn load_template_manifest(template_dir: &Path) -> MietteResult<TemplateManifest> {
        let manifest_path = template_dir.join("template.yaml");
        let raw = fs::read_to_string(&manifest_path).map_err(|err| {
            file_io_report(&manifest_path, "failed to read template manifest", err)
        })?;
        let manifest: TemplateManifest = serde_yaml::from_str(&raw)
            .map_err(|err| miette!("failed to parse '{}': {err}", manifest_path.display()))?;
        validate_template_manifest(&manifest, template_dir)?;
        Ok(manifest)
    }

    fn validate_template_manifest(
        manifest: &TemplateManifest,
        template_dir: &Path,
    ) -> MietteResult<()> {
        let dir_name =
            template_dir.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
                miette!("template path '{}' has no directory name", template_dir.display())
            })?;
        let ident = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$")
            .expect("template identifier regex should be valid");

        if manifest.name != dir_name {
            return Err(miette!(
                "template manifest name '{}' does not match directory '{}'",
                manifest.name,
                dir_name
            ));
        }
        if !ident.is_match(&manifest.name) {
            return Err(miette!("template name '{}' is not a valid identifier", manifest.name));
        }
        if manifest.description.trim().is_empty() {
            return Err(miette!(
                "template '{}' must include a non-empty description",
                manifest.name
            ));
        }

        let cwd = std::env::current_dir()
            .map_err(|err| miette!("failed to determine working directory: {err}"))?;
        let mut seen = HashSet::new();

        for input in &manifest.inputs {
            if !ident.is_match(&input.name) {
                return Err(miette!(
                    "template '{}' input '{}' is not a valid identifier",
                    manifest.name,
                    input.name
                ));
            }
            if !seen.insert(input.name.as_str()) {
                return Err(miette!(
                    "template '{}' declares duplicate input '{}'",
                    manifest.name,
                    input.name
                ));
            }
            if input.description.trim().is_empty() {
                return Err(miette!(
                    "template '{}' input '{}' must include a description",
                    manifest.name,
                    input.name
                ));
            }
            if input.required == Some(true) && input.default.is_some() {
                return Err(miette!(
                    "template '{}' input '{}' cannot set both required: true and default",
                    manifest.name,
                    input.name
                ));
            }
            if let Some(pattern) = input.validate.as_deref() {
                let _ = compile_full_match_regex(pattern).map_err(|err| {
                    miette!(
                        "template '{}' input '{}' has invalid validate regex: {err}",
                        manifest.name,
                        input.name
                    )
                })?;
            }
            if let Some(default) = input.default.as_ref() {
                let _ = coerce_template_input_value(input, default, &cwd, true)?;
            }
        }

        let _ = detect_template_layout(template_dir)?;

        Ok(())
    }

    fn detect_template_layout(template_dir: &Path) -> MietteResult<TemplateLayout> {
        let plan_path = template_dir.join("plan.rhei.md");
        let index_path = template_dir.join("index.rhei.md");
        let has_plan = plan_path.is_file();
        let has_index = index_path.is_file();

        match (has_plan, has_index) {
            (true, false) => Ok(TemplateLayout::SingleFile),
            (false, true) => {
                let tasks_dir = template_dir.join("tasks");
                if !tasks_dir.is_dir() {
                    return Err(miette!(
                        "template '{}' is a workspace template but is missing tasks/",
                        template_dir.display()
                    ));
                }
                Ok(TemplateLayout::Workspace)
            }
            (true, true) => Err(miette!(
                "template '{}' contains both plan.rhei.md and index.rhei.md",
                template_dir.display()
            )),
            (false, false) => Err(miette!(
                "template '{}' must contain either plan.rhei.md or index.rhei.md",
                template_dir.display()
            )),
        }
    }

    fn collect_template_inputs(
        manifest: &TemplateManifest,
        values_files: &[PathBuf],
        set_values: &[String],
        set_files: &[String],
    ) -> MietteResult<BTreeMap<String, String>> {
        let cwd = std::env::current_dir()
            .map_err(|err| miette!("failed to determine working directory: {err}"))?;
        let mut raw_values: BTreeMap<String, YamlValue> = BTreeMap::new();

        for values_file in values_files {
            let loaded = load_template_values_file(values_file)?;
            for (key, value) in loaded {
                raw_values.insert(key, value);
            }
        }

        for assignment in set_values {
            let (key, value) = parse_assignment(assignment, "--set")?;
            raw_values.insert(key, YamlValue::String(value));
        }

        for assignment in set_files {
            let (key, value_path) = parse_assignment(assignment, "--set-file")?;
            let path = PathBuf::from(value_path);
            let contents = fs::read_to_string(&path)
                .map_err(|err| file_io_report(&path, "failed to read --set-file input", err))?;
            raw_values.insert(key, YamlValue::String(contents));
        }

        let declared_inputs =
            manifest.inputs.iter().map(|input| input.name.as_str()).collect::<HashSet<_>>();
        for key in raw_values.keys() {
            if !declared_inputs.contains(key.as_str()) {
                return Err(miette!(
                    "template '{}' does not declare an input named '{}'",
                    manifest.name,
                    key
                ));
            }
        }

        let mut resolved = BTreeMap::new();
        for input in &manifest.inputs {
            let value = if let Some(raw) = raw_values.get(&input.name) {
                coerce_template_input_value(input, raw, &cwd, false)?
            } else if let Some(default) = input.default.as_ref() {
                coerce_template_input_value(input, default, &cwd, true)?
            } else if input.is_required() {
                return Err(miette!(
                    "template '{}' requires input '{}'",
                    manifest.name,
                    input.name
                ));
            } else {
                String::new()
            };

            if let Some(pattern) = input.validate.as_deref() {
                let regex = compile_full_match_regex(pattern).map_err(|err| {
                    miette!(
                        "template '{}' input '{}' has invalid validate regex: {err}",
                        manifest.name,
                        input.name
                    )
                })?;
                if !regex.is_match(&value) {
                    return Err(miette!(
                        "input '{}' does not match validation pattern '{}'",
                        input.name,
                        pattern
                    ));
                }
            }

            resolved.insert(input.name.clone(), value);
        }

        Ok(resolved)
    }

    fn load_template_values_file(path: &Path) -> MietteResult<BTreeMap<String, YamlValue>> {
        let raw = fs::read_to_string(path)
            .map_err(|err| file_io_report(path, "failed to read values file", err))?;
        if raw.trim().is_empty() {
            return Ok(BTreeMap::new());
        }

        let value: YamlValue = serde_yaml::from_str(&raw)
            .map_err(|err| miette!("failed to parse values file '{}': {err}", path.display()))?;
        let mapping = match value {
            YamlValue::Mapping(mapping) => mapping,
            _ => {
                return Err(miette!(
                    "values file '{}' must contain a YAML or JSON object at the top level",
                    path.display()
                ))
            }
        };

        let mut values = BTreeMap::new();
        for (key, value) in mapping {
            let Some(key) = key.as_str() else {
                return Err(miette!("values file '{}' contains a non-string key", path.display()));
            };
            values.insert(key.to_string(), value);
        }

        Ok(values)
    }

    fn parse_assignment(value: &str, flag_name: &str) -> MietteResult<(String, String)> {
        let Some((key, value)) = value.split_once('=') else {
            return Err(miette!("{} expects KEY=VALUE, got '{}'", flag_name, value));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(miette!("{} expects a non-empty key", flag_name));
        }
        Ok((key.to_string(), value.to_string()))
    }

    fn compile_full_match_regex(pattern: &str) -> Result<Regex> {
        Regex::new(&format!(r"\A(?:{})\z", pattern)).context("compile regex")
    }

    fn coerce_template_input_value(
        input: &TemplateInputDef,
        raw: &YamlValue,
        cwd: &Path,
        from_default: bool,
    ) -> MietteResult<String> {
        let source = if from_default { "default value" } else { "input value" };

        let rendered = match input.value_type {
            TemplateInputType::String => match raw {
                YamlValue::Null => String::new(),
                YamlValue::String(value) => value.clone(),
                _ => return Err(miette!("{} for '{}' must be a string", source, input.name)),
            },
            TemplateInputType::Number => match raw {
                YamlValue::Number(value) => value.to_string(),
                YamlValue::String(value) => {
                    let trimmed = value.trim();
                    let number_re = Regex::new(r"^-?\d+(?:\.\d+)?$")
                        .expect("number validation regex should be valid");
                    if !number_re.is_match(trimmed) {
                        return Err(miette!("{} for '{}' must be a number", source, input.name));
                    }
                    trimmed.to_string()
                }
                _ => return Err(miette!("{} for '{}' must be a number", source, input.name)),
            },
            TemplateInputType::Boolean => match raw {
                YamlValue::Bool(value) => value.to_string(),
                YamlValue::String(value) => match value.trim() {
                    "true" => "true".to_string(),
                    "false" => "false".to_string(),
                    _ => {
                        return Err(miette!(
                            "{} for '{}' must be true or false",
                            source,
                            input.name
                        ))
                    }
                },
                _ => return Err(miette!("{} for '{}' must be true or false", source, input.name)),
            },
            TemplateInputType::Path => match raw {
                YamlValue::String(value) => {
                    if value.is_empty() {
                        return Err(miette!("{} for '{}' must not be empty", source, input.name));
                    }
                    let path = PathBuf::from(value);
                    if path.is_absolute() { path } else { cwd.join(path) }.display().to_string()
                }
                _ => return Err(miette!("{} for '{}' must be a path string", source, input.name)),
            },
        };

        Ok(rendered)
    }

    fn print_template_inputs(manifest: &TemplateManifest) {
        println!("Template: {}", manifest.name);
        println!("Version: {}", manifest.version_string());
        println!("Description: {}", manifest.description);

        if manifest.inputs.is_empty() {
            println!("Inputs: none");
            return;
        }

        println!("Inputs:");
        for input in &manifest.inputs {
            let requirement = if input.is_required() {
                "required".to_string()
            } else if let Some(default) = input.default.as_ref() {
                format!("default={}", format_version(default))
            } else {
                "optional".to_string()
            };
            println!("  {} ({}, {})", input.name, input.value_type.as_str(), requirement);
            println!("    {}", input.description);
            if let Some(pattern) = input.validate.as_deref() {
                println!("    validate: {}", pattern);
            }
        }
    }

    fn materialize_template(
        template_dir: &Path,
        layout: TemplateLayout,
        output_dir: &Path,
        values: &BTreeMap<String, String>,
    ) -> MietteResult<MaterializedTemplate> {
        fs::create_dir_all(output_dir)
            .map_err(|err| file_io_report(output_dir, "failed to create output directory", err))?;
        let root_permissions = fs::metadata(template_dir)
            .map_err(|err| file_io_report(template_dir, "failed to read template metadata", err))?
            .permissions();
        fs::set_permissions(output_dir, root_permissions).map_err(|err| {
            file_io_report(output_dir, "failed to preserve output directory permissions", err)
        })?;

        let mut generated_files = Vec::new();
        materialize_template_dir(
            template_dir,
            output_dir,
            template_dir,
            values,
            &mut generated_files,
        )?;

        Ok(MaterializedTemplate { layout, output_dir: output_dir.to_path_buf(), generated_files })
    }

    fn materialize_template_dir(
        src_dir: &Path,
        dest_dir: &Path,
        template_root: &Path,
        values: &BTreeMap<String, String>,
        generated_files: &mut Vec<PathBuf>,
    ) -> MietteResult<()> {
        let mut entries = fs::read_dir(src_dir)
            .map_err(|err| file_io_report(src_dir, "failed to read template directory", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!("failed to read dir entry in '{}': {err}", src_dir.display()))?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }

            let src_path = entry.path();
            if src_path == template_root.join("template.yaml") {
                continue;
            }

            // A root-level `settings.json` in the template is relocated to
            // `.rhei/settings.json` in the output, where `rhei run` and
            // `rhei validate` pick it up as project-scoped settings. Any
            // non-root `settings.json` is left where it is.
            let at_template_root = src_dir == template_root;
            let dest_path = if at_template_root && name_str == "settings.json" {
                let rhei_dir = dest_dir.join(".rhei");
                fs::create_dir_all(&rhei_dir).map_err(|err| {
                    file_io_report(&rhei_dir, "failed to create .rhei directory", err)
                })?;
                rhei_dir.join("settings.json")
            } else {
                dest_dir.join(&name)
            };
            let metadata = entry.metadata().map_err(|err| {
                file_io_report(&src_path, "failed to read template metadata", err)
            })?;

            if metadata.is_dir() {
                fs::create_dir_all(&dest_path).map_err(|err| {
                    file_io_report(&dest_path, "failed to create output directory", err)
                })?;
                fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                    file_io_report(&dest_path, "failed to preserve directory permissions", err)
                })?;
                materialize_template_dir(
                    &src_path,
                    &dest_path,
                    template_root,
                    values,
                    generated_files,
                )?;
                continue;
            }

            if is_text_template_file(&src_path)? {
                let raw = fs::read_to_string(&src_path).map_err(|err| {
                    file_io_report(&src_path, "failed to read template text file", err)
                })?;
                let rendered = render_template_text(&raw, values, &src_path)?;
                // Template-shipped settings.json must parse as JSON after
                // instantiation-variable substitution. Catching this here
                // surfaces malformed bundles before `rhei validate` runs.
                if at_template_root && name_str == "settings.json" {
                    serde_json::from_str::<serde_json::Value>(&rendered).map_err(|err| {
                        miette!(
                            "template settings.json is not valid JSON after instantiation: {err}"
                        )
                    })?;
                }
                fs::write(&dest_path, rendered).map_err(|err| {
                    file_io_report(&dest_path, "failed to write output file", err)
                })?;
            } else {
                fs::copy(&src_path, &dest_path).map_err(|err| {
                    miette!(
                        "failed to copy '{}' to '{}': {err}",
                        src_path.display(),
                        dest_path.display()
                    )
                })?;
            }

            fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                file_io_report(&dest_path, "failed to preserve file permissions", err)
            })?;
            generated_files.push(
                dest_path
                    .strip_prefix(dest_dir.ancestors().last().unwrap_or(dest_dir))
                    .unwrap_or(&dest_path)
                    .to_path_buf(),
            );
        }

        Ok(())
    }

    fn is_text_template_file(path: &Path) -> MietteResult<bool> {
        let bytes = fs::read(path)
            .map_err(|err| file_io_report(path, "failed to read template file", err))?;
        Ok(!bytes[..bytes.len().min(8192)].contains(&0))
    }

    fn render_template_text(
        raw: &str,
        values: &BTreeMap<String, String>,
        path: &Path,
    ) -> MietteResult<String> {
        let mut rendered = String::with_capacity(raw.len());
        let mut idx = 0usize;

        while idx < raw.len() {
            let slice = &raw[idx..];
            if slice.starts_with(r"\{{") {
                rendered.push_str("{{");
                idx += 3;
                continue;
            }

            if slice.starts_with("{{") {
                let rest = &raw[idx + 2..];
                let Some(end) = rest.find("}}") else {
                    return Err(miette!("unclosed template variable in '{}'", path.display()));
                };
                let token = rest[..end].trim();
                let value = values.get(token).ok_or_else(|| {
                    miette!(
                        "unresolved template variable '{{{{{}}}}}' in '{}'",
                        token,
                        path.display()
                    )
                })?;
                rendered.push_str(value);
                idx += 2 + end + 2;
                continue;
            }

            let ch =
                slice.chars().next().expect("slice should always contain at least one character");
            rendered.push(ch);
            idx += ch.len_utf8();
        }

        Ok(rendered)
    }
}

fn render_state_machine_text(machine: &rhei_validator::StateMachine) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "State machine: {} (version: {})\n",
        machine.name,
        format_version(&machine.version)
    ));
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
            if let Some(visits) = def.visits {
                out.push_str(&format!("      Visits: {visits}\n"));
            }
            if !def.all_models.is_empty() {
                out.push_str(&format!("      Models: {}\n", def.all_models.join(", ")));
            } else if let Some(model) = def.model.as_deref() {
                out.push_str(&format!("      Model: {model}\n"));
            }
            if !def.inputs.is_empty() {
                out.push_str("      Inputs:\n");
                for artifact in &def.inputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if !def.outputs.is_empty() {
                out.push_str("      Outputs:\n");
                for artifact in &def.outputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
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
                "visits": def.visits,
                "all_models": def.all_models,
                "model": def.model,
                "inputs": def.inputs,
                "outputs": def.outputs,
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
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine)?;
    let base_path = input.parent().unwrap_or(Path::new("."));
    let report =
        rhei_validator::validate_with_machine_and_base(&loaded.rhei, &resolved.machine, base_path);

    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
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
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine)?;
    let watched_paths = match resolved.path.as_deref() {
        Some(sm) => canonical_watched_paths(input, sm),
        None => canonical_watched_paths(input, input), // only watch the plan itself
    };
    let watch_roots = match resolved.path.as_deref() {
        Some(sm) => watch_roots(input, sm),
        None => watch_roots(input, input),
    };

    println!(
        "Watch mode started for '{}' (states: {})",
        input.display(),
        state_machine_label(resolved.path.as_deref()),
    );

    run_validation_pass(input, state_machine);

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
        run_validation_pass(input, state_machine);
    }
}

/// Run one validation pass in watch mode, writing any failure to stderr.
fn run_validation_pass(input: &Path, state_machine: Option<&Path>) {
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

fn task_visit_count(metadata: Option<&Metadata>, task_id: &TaskId, state_name: &str) -> u64 {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key("stateVisits")))
        .and_then(YamlValue::as_mapping)
        .and_then(|state_visits| state_visits.get(yaml_key(state_name)))
        .and_then(yaml_value_to_u64)
        .map(|count| count.max(1))
        .unwrap_or(0)
}

fn parsed_task_state(
    raw_state: &str,
    machine: &rhei_validator::StateMachine,
) -> rhei_validator::ParsedTaskState {
    rhei_validator::parse_task_state(raw_state, machine)
}

fn normalized_state_name(raw_state: &str, machine: &rhei_validator::StateMachine) -> String {
    parsed_task_state(raw_state, machine).state
}

fn raw_state_visit_count(
    raw_state: &str,
    machine: &rhei_validator::StateMachine,
    expected_state: &str,
) -> u64 {
    let parsed = parsed_task_state(raw_state, machine);
    if parsed.state != expected_state || state_visit_limit(machine, expected_state).is_none() {
        return 0;
    }

    parsed.visit.map(u64::from).unwrap_or(1)
}

fn format_task_state_value(
    state_name: &str,
    visit_count: Option<u64>,
    machine: &rhei_validator::StateMachine,
) -> String {
    match visit_count.filter(|count| *count > 1) {
        Some(count) if state_visit_limit(machine, state_name).is_some() => {
            format!("{state_name}-{count}")
        }
        _ => state_name.to_string(),
    }
}

fn format_state_metadata_value(raw_state: &str) -> String {
    if raw_state.starts_with('`') && raw_state.ends_with('`') {
        raw_state.to_string()
    } else if raw_state.contains(' ') {
        format!("`{raw_state}`")
    } else {
        raw_state.to_string()
    }
}

fn state_visit_limit(machine: &rhei_validator::StateMachine, state_name: &str) -> Option<u64> {
    machine.states.get(state_name).and_then(|def| def.visits).map(u64::from)
}

fn current_state_visit_count(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> u64 {
    let current = task_visit_count(metadata, task_id, current_state).max(raw_state_visit_count(
        current_state_raw,
        machine,
        current_state,
    ));
    if current > 0 {
        return current;
    }

    if state_visit_limit(machine, current_state).is_some() {
        return 1;
    }

    0
}

fn resolve_condition_operand(
    token: &str,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<i64> {
    if let Ok(value) = token.parse::<i64>() {
        return Ok(value);
    }

    match token {
        "visitCount" | "visit_count" => Ok(current_state_visit_count(
            metadata,
            task_id,
            current_state,
            current_state_raw,
            machine,
        ) as i64),
        "visits" => {
            let limit = state_visit_limit(machine, current_state).ok_or_else(|| {
                miette!("state '{}' does not declare a visit limit", current_state)
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
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<bool> {
    let parts = condition.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(miette!(
            "unsupported transition condition '{}'; expected '<lhs> <op> <rhs>'",
            condition
        ));
    }

    let lhs = resolve_condition_operand(
        parts[0],
        metadata,
        task_id,
        current_state,
        current_state_raw,
        machine,
    )?;
    let rhs = resolve_condition_operand(
        parts[2],
        metadata,
        task_id,
        current_state,
        current_state_raw,
        machine,
    )?;

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
    current_state: &str,
    current_state_raw: &str,
    to_state: &str,
) -> bool {
    let Some(limit) = state_visit_limit(machine, to_state) else {
        return true;
    };

    let mut current = task_visit_count(metadata, task_id, to_state);
    if current_state == to_state {
        current = current.max(raw_state_visit_count(current_state_raw, machine, to_state));
    }
    current < limit
}

fn transition_rule_is_applicable(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
) -> MietteResult<bool> {
    if !loop_reentry_allowed(
        machine,
        metadata,
        task_id,
        current_state,
        current_state_raw,
        &rule.to.0,
    ) {
        return Ok(false);
    }

    if let Some(condition) = rule.condition.as_deref() {
        return evaluate_transition_condition(
            condition,
            metadata,
            task_id,
            current_state,
            current_state_raw,
            machine,
        );
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

fn ensure_current_state_visit_count(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<Metadata> {
    state_visit_limit(machine, current_state)?;

    let current =
        current_state_visit_count(existing, task_id, current_state, current_state_raw, machine);
    if current == task_visit_count(existing, task_id, current_state) {
        return existing.cloned();
    }

    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let state_visits = ensure_mapping(task_entry, yaml_key("stateVisits"));
    state_visits.insert(yaml_key(current_state), yaml_u64(current));
    Some(root)
}

fn update_metadata_for_transition(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    to_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<Metadata> {
    state_visit_limit(machine, to_state)?;

    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let state_visits = ensure_mapping(task_entry, yaml_key("stateVisits"));
    let state_key = yaml_key(to_state);
    let next =
        state_visits.get(&state_key).and_then(yaml_value_to_u64).map(|n| n.max(1) + 1).unwrap_or(1);
    state_visits.insert(state_key, yaml_u64(next));
    Some(root)
}

fn clear_runtime_state_visits(existing: Option<&Metadata>) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata_section)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata_section.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };

    for value in tasks.values_mut() {
        if let YamlValue::Mapping(task_map) = value {
            task_map.remove(yaml_key("stateVisits"));
        }
    }

    Some(root)
}

struct CallbackPaths {
    plan_path: PathBuf,
    state_machine_path: Option<PathBuf>,
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
    let state_machine_path = state_machine_path
        .map(|path| {
            path.canonicalize().map_err(|err| {
                file_io_report(path, "failed to resolve state machine path for callbacks", err)
            })
        })
        .transpose()?;
    let base_dir = if let Some(path) = state_machine_path.as_deref() {
        path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
    } else if plan_path.is_dir() {
        plan_path.as_path()
    } else {
        plan_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
    };

    let working_dir = base_dir.canonicalize().map_err(|err| {
        file_io_report(base_dir, "failed to resolve callback working directory", err)
    })?;

    Ok(CallbackPaths { plan_path, state_machine_path, working_dir })
}

fn execution_workspace_root(plan_path: &Path) -> PathBuf {
    if plan_path.is_dir() {
        plan_path.to_path_buf()
    } else {
        plan_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}

struct RuntimeTemplateContext<'a> {
    workspace_root: &'a Path,
    plan_path: &'a Path,
    state_machine_path: Option<&'a Path>,
    plan_title: &'a str,
    task: &'a rhei_core::ast::Task,
    state_name: &'a str,
    current_state_raw: &'a str,
    machine: &'a rhei_validator::StateMachine,
    metadata: Option<&'a Metadata>,
    model: Option<&'a str>,
    agent: Option<&'a str>,
    /// Resolved MCP servers and skills for the current state (Half A).
    /// Availability here reflects registry resolution only; Half B will
    /// overlay real handshake results.
    tooling: Option<&'a ResolvedTooling>,
}

fn yaml_value_to_template_string(value: &YamlValue) -> Option<String> {
    match value {
        YamlValue::Null => Some(String::new()),
        YamlValue::Bool(value) => Some(value.to_string()),
        YamlValue::Number(value) => Some(value.to_string()),
        YamlValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn render_visit_count(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> u64 {
    let visit =
        current_state_visit_count(metadata, task_id, state_name, current_state_raw, machine);
    visit.max(1)
}

fn artifact_relative_path(
    artifact: &rhei_validator::StateArtifactDef,
    task_id: &str,
    state_name: &str,
    visit_count: Option<u64>,
    model: Option<&str>,
) -> String {
    let mut relative = artifact.path.replace("{task_id}", task_id).replace("{state}", state_name);
    if let Some(visit_count) = visit_count {
        relative = relative.replace("{visit_count}", &visit_count.to_string());
    }
    if let Some(model) = model {
        relative = relative.replace("{model}", model);
    }
    relative
}

fn resolve_artifact_path(
    workspace_root: &Path,
    artifact: &rhei_validator::StateArtifactDef,
    task_id: &str,
    state_name: &str,
    visit_count: Option<u64>,
    model: Option<&str>,
) -> (String, PathBuf) {
    let relative = artifact_relative_path(artifact, task_id, state_name, visit_count, model);
    (relative.clone(), workspace_root.join(&relative))
}

fn resolve_runtime_template_variable(
    variable: &str,
    context: &RuntimeTemplateContext<'_>,
) -> Option<String> {
    match variable {
        "task_id" => Some(context.task.id.to_string()),
        "task_title" => Some(context.task.title.clone()),
        "state" => Some(context.state_name.to_string()),
        "visit_count" => Some(
            render_visit_count(
                context.metadata,
                &context.task.id,
                context.state_name,
                context.current_state_raw,
                context.machine,
            )
            .to_string(),
        ),
        "visits" => state_visit_limit(context.machine, context.state_name).map(|n| n.to_string()),
        "model" => context.model.map(str::to_string),
        "agent" => context.agent.map(str::to_string),
        "plan_title" => Some(context.plan_title.to_string()),
        "plan_path" => Some(context.plan_path.display().to_string()),
        _ => {
            if let Some(key) = variable.strip_prefix("meta.") {
                return task_metadata_map(context.metadata, &context.task.id)
                    .and_then(|task_map| task_map.get(yaml_key(key)))
                    .and_then(yaml_value_to_template_string);
            }

            let visit_count = Some(render_visit_count(
                context.metadata,
                &context.task.id,
                context.state_name,
                context.current_state_raw,
                context.machine,
            ));
            let state_def = context.machine.states.get(context.state_name)?;

            if let Some(name) =
                variable.strip_prefix("input.").and_then(|v| v.strip_suffix(".path"))
            {
                return state_def.inputs.iter().find(|artifact| artifact.name == name).map(
                    |artifact| {
                        artifact_relative_path(
                            artifact,
                            &context.task.id.to_string(),
                            context.state_name,
                            visit_count,
                            context.model,
                        )
                    },
                );
            }

            if let Some(name) =
                variable.strip_prefix("input.").and_then(|v| v.strip_suffix(".exists"))
            {
                return state_def.inputs.iter().find(|artifact| artifact.name == name).map(
                    |artifact| {
                        let (_, path) = resolve_artifact_path(
                            context.workspace_root,
                            artifact,
                            &context.task.id.to_string(),
                            context.state_name,
                            visit_count,
                            context.model,
                        );
                        path.exists().to_string()
                    },
                );
            }

            if let Some(name) =
                variable.strip_prefix("output.").and_then(|v| v.strip_suffix(".path"))
            {
                return state_def.outputs.iter().find(|artifact| artifact.name == name).map(
                    |artifact| {
                        artifact_relative_path(
                            artifact,
                            &context.task.id.to_string(),
                            context.state_name,
                            visit_count,
                            context.model,
                        )
                    },
                );
            }

            if let Some(name) =
                variable.strip_prefix("mcp.").and_then(|v| v.strip_suffix(".available"))
            {
                return context.tooling.map(|t| t.mcp_available(name).to_string());
            }

            if let Some(id) =
                variable.strip_prefix("skill.").and_then(|v| v.strip_suffix(".available"))
            {
                return context.tooling.map(|t| t.skill_available(id).to_string());
            }

            None
        }
    }
}

/// Evaluate a condition expression for `{if <condition>}` blocks.
///
/// Supported forms: `input.<name>.exists`, `mcp.<name>.available`,
/// `skill.<id>.available`.
fn evaluate_if_condition(condition: &str, context: &RuntimeTemplateContext<'_>) -> bool {
    if let Some(name) = condition.strip_prefix("input.").and_then(|s| s.strip_suffix(".exists")) {
        let visit_count = Some(render_visit_count(
            context.metadata,
            &context.task.id,
            context.state_name,
            context.current_state_raw,
            context.machine,
        ));
        if let Some(state_def) = context.machine.states.get(context.state_name) {
            if let Some(artifact) = state_def.inputs.iter().find(|a| a.name == name) {
                let (_, path) = resolve_artifact_path(
                    context.workspace_root,
                    artifact,
                    &context.task.id.to_string(),
                    context.state_name,
                    visit_count,
                    context.model,
                );
                return path.exists();
            }
        }
        return false;
    }

    if let Some(name) = condition.strip_prefix("mcp.").and_then(|s| s.strip_suffix(".available")) {
        return context.tooling.map_or(false, |t| t.mcp_available(name));
    }

    if let Some(id) = condition.strip_prefix("skill.").and_then(|s| s.strip_suffix(".available")) {
        return context.tooling.map_or(false, |t| t.skill_available(id));
    }

    false
}

/// Parse the body of an `{if}` block (text after the opening `{if ...}\n`).
///
/// Returns `(true_branch, optional_false_branch, text_after_endif)`.
/// Tag lines (`{else}`, `{endif}`) are consumed and excluded from all slices.
fn parse_if_block(body: &str) -> (&str, Option<&str>, &str) {
    if let Some(else_pos) = body.find("{else}") {
        let true_branch = &body[..else_pos];
        let after_else_tag = else_pos + "{else}".len();
        let false_start = if body[after_else_tag..].starts_with('\n') {
            after_else_tag + 1
        } else {
            after_else_tag
        };
        if let Some(endif_rel) = body[false_start..].find("{endif}") {
            let false_branch = &body[false_start..false_start + endif_rel];
            let after_endif_tag = false_start + endif_rel + "{endif}".len();
            let after_endif = if body[after_endif_tag..].starts_with('\n') {
                &body[after_endif_tag + 1..]
            } else {
                &body[after_endif_tag..]
            };
            return (true_branch, Some(false_branch), after_endif);
        }
        // Malformed: {else} but no {endif} — treat whole body as true branch
        return (body, None, "");
    }

    if let Some(endif_pos) = body.find("{endif}") {
        let true_branch = &body[..endif_pos];
        let after_endif_tag = endif_pos + "{endif}".len();
        let after_endif = if body[after_endif_tag..].starts_with('\n') {
            &body[after_endif_tag + 1..]
        } else {
            &body[after_endif_tag..]
        };
        return (true_branch, None, after_endif);
    }

    // Malformed: no {endif} — treat whole body as true branch
    (body, None, "")
}

/// Collapse runs of three or more consecutive newlines to exactly two.
///
/// Two newlines (`\n\n`) represent a single blank line in prose. When a
/// conditional block is removed, adjacent blank lines from the surrounding text
/// and the removed block merge into a run of 3+; this collapses them back to
/// one blank line so the output stays clean.
fn collapse_extra_blank_lines(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut newline_run = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                result.push(ch);
            }
        } else {
            newline_run = 0;
            result.push(ch);
        }
    }
    result
}

/// Pre-pass over `text` that resolves `{if condition}…{else}…{endif}` blocks
/// before variable substitution runs.
///
/// Supported conditions (v1): `input.<name>.exists`
/// Nesting is not supported in v1.
fn process_conditional_blocks(text: &str, context: &RuntimeTemplateContext<'_>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(if_start) = remaining.find("{if ") {
        let after_open = if_start + "{if ".len();
        let Some(close_brace) = remaining[after_open..].find('}') else {
            // Malformed opening tag — pass through the '{' and move on
            result.push_str(&remaining[..if_start + 1]);
            remaining = &remaining[if_start + 1..];
            continue;
        };
        let condition = &remaining[after_open..after_open + close_brace];
        let tag_end = after_open + close_brace + 1; // position after '}'

        // Consume the newline that follows the opening tag line
        let body_start = if remaining[tag_end..].starts_with('\n') { tag_end + 1 } else { tag_end };

        // Emit everything before the opening tag unchanged
        result.push_str(&remaining[..if_start]);

        let (true_branch, false_branch, after_endif) = parse_if_block(&remaining[body_start..]);

        if evaluate_if_condition(condition, context) {
            result.push_str(true_branch);
        } else if let Some(fb) = false_branch {
            result.push_str(fb);
        }
        // else: block removed entirely

        remaining = after_endif;
    }

    result.push_str(remaining);
    collapse_extra_blank_lines(&result)
}

fn resolve_runtime_template_text(text: &str, context: &RuntimeTemplateContext<'_>) -> String {
    let preprocessed = process_conditional_blocks(text, context);
    let text = preprocessed.as_str();
    let mut rendered = String::with_capacity(text.len());
    let mut idx = 0usize;

    while idx < text.len() {
        if !text[idx..].starts_with('{') {
            let ch = text[idx..].chars().next().expect("substring should have a char");
            rendered.push(ch);
            idx += ch.len_utf8();
            continue;
        }

        let mut end = idx + 1;
        while end < text.len() && !text[end..].starts_with('}') {
            end += 1;
        }
        if end >= text.len() {
            rendered.push('{');
            idx += 1;
            continue;
        }

        let token = &text[idx + 1..end];
        if let Some(value) = resolve_runtime_template_variable(token, context) {
            rendered.push_str(&value);
        } else {
            rendered.push_str(&text[idx..=end]);
        }
        idx = end + 1;
    }

    rendered
}

fn ensure_state_inputs_exist(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    model: Option<&str>,
    context: &str,
) -> MietteResult<()> {
    for artifact in &state_def.inputs {
        if artifact.optional {
            continue;
        }
        let (relative, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            task_id,
            state_name,
            visit_count,
            model,
        );
        if !path.exists() {
            return Err(miette!(
                "{context}\nMissing required input artifact: {} ({})",
                artifact.name,
                relative
            ));
        }
    }

    Ok(())
}

fn ensure_state_outputs_exist(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    model: Option<&str>,
) -> MietteResult<()> {
    for artifact in &state_def.outputs {
        let (relative, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            task_id,
            state_name,
            visit_count,
            model,
        );
        if !path.exists() {
            return Err(miette!(
                "Task {} cannot leave state {}.\nMissing required output artifact: {} ({})",
                task_id,
                state_name,
                artifact.name,
                relative
            ));
        }
    }

    Ok(())
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
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;

    let task_file = if workspace::is_workspace(input) {
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
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);

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
    let current_state_raw = find_task_current_state(&task_raw, task_file, &target_id, task_id_str)?;
    let current_state = normalized_state_name(&current_state_raw, machine);
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
            current_state_raw,
            from
        ));
    }

    let normalized_metadata = ensure_current_state_visit_count(
        metadata.as_ref(),
        &target_id,
        from,
        &current_state_raw,
        machine,
    );
    let metadata_for_checks = normalized_metadata.as_ref().or(metadata.as_ref());

    if !transition_rule_is_applicable(
        matching_rule,
        machine,
        metadata_for_checks,
        &target_id,
        from,
        &current_state_raw,
    )? {
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

    let from_state_def = machine
        .states
        .get(from)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", from))?;
    let to_state_def = machine
        .states
        .get(to)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", to))?;

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
                    agent: None,
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
        update_metadata_for_transition(metadata_for_checks, &target_id, to, machine)
            .or_else(|| normalized_metadata.clone());
    let from_visit_count = Some(render_visit_count(
        metadata_for_checks,
        &target_id,
        from,
        &current_state_raw,
        machine,
    ));
    let to_visit_count = updated_metadata
        .as_ref()
        .map(|meta| task_visit_count(Some(meta), &target_id, to))
        .filter(|count| *count > 0);

    ensure_state_outputs_exist(
        &workspace_root,
        task_id_str,
        from,
        from_state_def,
        from_visit_count,
        None,
    )?;
    ensure_state_inputs_exist(
        &workspace_root,
        task_id_str,
        to,
        to_state_def,
        to_visit_count,
        None,
        &format!("Task {} cannot enter state {}.", task_id_str, to),
    )?;

    let rendered_to_state = format_task_state_value(to, to_visit_count, machine);
    let metadata_raw_updated = if task_file == metadata_file {
        let new_task_raw = rewrite_task_state(&task_raw, task_id_str, &rendered_to_state)?;
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
        Some(rewrite_task_state(&task_raw, task_id_str, &rendered_to_state)?)
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
        agent: None,
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

// ─── Agent Configuration ──────────────────────────────────────────────

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
    #[arg(long, default_value_t = 1)]
    parallel: usize,
}

/// Flags that control agent-specific behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Agent Execution")]
struct AgentExecutionFlags {
    /// Disable agent spawning; use callback-only advancement
    #[arg(long)]
    no_agent: bool,
    /// Override the agent for this run
    #[arg(long, value_name = "AGENT")]
    agent: Option<String>,
    /// Override the agent mode (named flag set) for this run
    #[arg(long, value_name = "MODE")]
    agent_mode: Option<String>,
    /// Override the model for this run
    #[arg(long, value_name = "MODEL")]
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
    #[arg(long, value_name = "DURATION")]
    program_timeout: Option<String>,
}

/// Options for the `run` command.
struct RunOptions {
    standalone: StandaloneExecutionFlags,
    agent: AgentExecutionFlags,
    program: ProgramExecutionFlags,
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
}

impl From<(StandaloneExecutionFlags, AgentExecutionFlags, ProgramExecutionFlags)> for RunOptions {
    fn from(
        (standalone, agent, program): (
            StandaloneExecutionFlags,
            AgentExecutionFlags,
            ProgramExecutionFlags,
        ),
    ) -> Self {
        Self { standalone, agent, program }
    }
}

/// Rhei settings loaded from `~/.config/rhei/settings.json` or `.rhei/settings.json`.
#[derive(Debug, Default, Deserialize)]
struct RheiSettings {
    #[serde(default)]
    agent: Option<AgentConfig>,
    #[serde(default)]
    agent_mode: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    agent_timeout: Option<String>,
    #[serde(default)]
    program_timeout: Option<String>,
    /// Spec-aligned nested defaults. Only `mcp_servers` and `skills` are read here
    /// today; `model` / `agent` remain readable at the top level for backward
    /// compatibility.
    #[serde(default)]
    defaults: SettingsDefaults,
    /// Registry of agent transport profiles keyed by agent id.
    #[serde(default)]
    agents: BTreeMap<String, CustomAgentProfile>,
    /// Registry of MCP server profiles keyed by server id.
    #[serde(default)]
    mcp_servers: BTreeMap<String, McpServerProfile>,
    /// Registry of skill profiles keyed by skill id.
    #[serde(default)]
    skills: BTreeMap<String, SkillProfile>,
}

/// Nested `defaults` section in settings.
///
/// `mcp_servers` and `skills` use `Option<Vec<_>>` so the merge layer can
/// distinguish "unset" (inherit) from "empty" (explicitly clear inherited).
#[derive(Debug, Default, Deserialize, Clone)]
struct SettingsDefaults {
    /// Default agent mode applied when a state does not set `agent_mode`.
    /// `null` explicitly clears an inherited default.
    #[serde(default)]
    agent_mode: Option<String>,
    #[serde(default)]
    mcp_servers: Option<Vec<StateMcpEntry>>,
    #[serde(default)]
    skills: Option<Vec<StateSkillEntry>>,
}

/// Built-in agent registry.
///
/// Each entry is a ready-to-use `CustomAgentProfile` for one of the agents
/// that Rhei supports out of the box. The per-agent "autonomous" flag set
/// that was hard-coded as `default_args` is now exposed as a named `yolo`
/// mode so states and defaults can select it explicitly via `agent_mode`.
///
/// A user-written entry with the same id in global or project settings
/// replaces the built-in entry wholesale (see `load_merged_settings`).
fn built_in_agents() -> BTreeMap<String, CustomAgentProfile> {
    fn flags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    let modes_yolo_only = |yolo: Vec<String>| {
        let mut modes = BTreeMap::new();
        modes.insert("yolo".to_string(), yolo);
        modes
    };

    let mut agents = BTreeMap::new();

    agents.insert(
        "claude-code".to_string(),
        CustomAgentProfile {
            command: flags(&["claude"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            mcp_config_flag: Some("--mcp-config".to_string()),
            skill_flag: Some("--skill".to_string()),
            modes: modes_yolo_only(flags(&["--permission-mode", "bypassPermissions"])),
            ..Default::default()
        },
    );

    agents.insert(
        "codex".to_string(),
        CustomAgentProfile {
            command: flags(&["codex", "exec"]),
            prompt_flag: None,
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            mcp_flag: Some("--mcp".to_string()),
            modes: modes_yolo_only(flags(&[
                "--sandbox",
                "danger-full-access",
                "--skip-git-repo-check",
            ])),
            ..Default::default()
        },
    );

    agents.insert(
        "gemini".to_string(),
        CustomAgentProfile {
            command: flags(&["gemini"]),
            prompt_flag: Some("--prompt".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--approval-mode", "auto_edit"])),
            ..Default::default()
        },
    );

    agents.insert(
        "kilocode".to_string(),
        CustomAgentProfile {
            command: flags(&["kilo"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            ..Default::default()
        },
    );

    agents.insert(
        "cursor".to_string(),
        CustomAgentProfile {
            command: flags(&["cursor"]),
            prompt_flag: Some("--prompt".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            ..Default::default()
        },
    );

    agents
}

/// Load settings from a JSON file, returning defaults if the file doesn't exist.
fn load_settings(path: &Path) -> RheiSettings {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => RheiSettings::default(),
    }
}

/// Load merged settings: built-ins, then global, then project-level overrides.
fn load_merged_settings(plan_root: &Path) -> RheiSettings {
    let global = home_dir()
        .map(|h| h.join(".config/rhei/settings.json"))
        .map(|p| load_settings(&p))
        .unwrap_or_default();

    let project = load_settings(&plan_root.join(".rhei/settings.json"));

    // Agent registry: built-ins seed the map; global then project entries
    // replace an id wholesale when present.
    let mut agents = built_in_agents();
    for (id, profile) in global.agents {
        agents.insert(id, profile);
    }
    for (id, profile) in project.agents {
        agents.insert(id, profile);
    }

    // Registries merge by id: start with global, override by project.
    let mut mcp_servers = global.mcp_servers.clone();
    for (id, profile) in project.mcp_servers {
        mcp_servers.insert(id, profile);
    }
    let mut skills = global.skills.clone();
    for (id, profile) in project.skills {
        skills.insert(id, profile);
    }

    // `defaults.mcp_servers` / `defaults.skills`: project replaces global
    // wholesale when present (including an explicit empty list).
    let defaults = SettingsDefaults {
        agent_mode: project.defaults.agent_mode.or(global.defaults.agent_mode),
        mcp_servers: project.defaults.mcp_servers.or(global.defaults.mcp_servers),
        skills: project.defaults.skills.or(global.defaults.skills),
    };

    RheiSettings {
        agent: project.agent.or(global.agent),
        agent_mode: project.agent_mode.or(global.agent_mode),
        model: project.model.or(global.model),
        agent_timeout: project.agent_timeout.or(global.agent_timeout),
        program_timeout: project.program_timeout.or(global.program_timeout),
        defaults,
        agents,
        mcp_servers,
        skills,
    }
}

/// One fully-resolved MCP server entry in a state's effective set.
///
/// `definition` is `Some` when the entry resolves against the merged registry
/// or carries inline fields, and `None` only when the id is unknown — callers
/// treat the latter as a validation error.
#[derive(Debug, Clone)]
struct ResolvedMcpEntry {
    id: String,
    /// `optional: true` on the declaring entry. Used by Half B to decide
    /// whether a failed availability check blocks the agent or is downgraded
    /// to a warning. Carried in Half A so the resolution path is complete.
    #[allow(dead_code)]
    optional: bool,
    definition: Option<McpServerProfile>,
}

/// One fully-resolved skill entry in a state's effective set.
#[derive(Debug, Clone)]
struct ResolvedSkillEntry {
    id: String,
    #[allow(dead_code)]
    optional: bool,
    definition: Option<SkillProfile>,
}

/// The tooling a state contributes to the agent subprocess.
///
/// Half A: availability is computed from registry resolution only — an entry
/// whose id resolves (or carries an inline definition) is reported available.
/// Half B will hook actual MCP handshake checks and skill-path probes into
/// the same struct, leaving call sites unchanged.
#[derive(Debug, Clone, Default)]
struct ResolvedTooling {
    mcp_servers: Vec<ResolvedMcpEntry>,
    skills: Vec<ResolvedSkillEntry>,
}

impl ResolvedTooling {
    /// Ids whose definition resolved — used for `{mcp.<name>.available}` and
    /// the `RHEI_MCP_<NAME>_AVAILABLE` env vars.
    fn mcp_available(&self, id: &str) -> bool {
        self.mcp_servers.iter().any(|e| e.id == id && e.definition.is_some())
    }

    fn skill_available(&self, id: &str) -> bool {
        self.skills.iter().any(|e| e.id == id && e.definition.is_some())
    }

    /// Comma-separated ids of resolved MCP servers (available only).
    fn mcp_servers_csv(&self) -> String {
        self.mcp_servers
            .iter()
            .filter(|e| e.definition.is_some())
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn skills_csv(&self) -> String {
        self.skills
            .iter()
            .filter(|e| e.definition.is_some())
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Normalize an id into the env-var segment used by `RHEI_*_<NAME>_AVAILABLE`.
fn env_id_segment(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect()
}

/// Compute the effective tooling set for a state given the merged settings.
fn resolve_tooling(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
) -> ResolvedTooling {
    let state_def = machine.states.get(state_name);

    // MCP: start from defaults (if any), then override/extend with state-level.
    let mcp_entries = effective_mcp_entries(
        settings.defaults.mcp_servers.as_deref().unwrap_or(&[]),
        state_def.and_then(|d| d.mcp_servers.as_deref()),
    );
    let mcp_servers: Vec<ResolvedMcpEntry> = mcp_entries
        .into_iter()
        .map(|entry| resolve_mcp_entry(&entry, &settings.mcp_servers))
        .collect();

    let skill_entries = effective_skill_entries(
        settings.defaults.skills.as_deref().unwrap_or(&[]),
        state_def.and_then(|d| d.skills.as_deref()),
    );
    let skills: Vec<ResolvedSkillEntry> = skill_entries
        .into_iter()
        .map(|entry| resolve_skill_entry(&entry, &settings.skills))
        .collect();

    ResolvedTooling { mcp_servers, skills }
}

/// Union `defaults.mcp_servers` with a state's `mcp_servers`, deduped by id.
///
/// `None` on the state = inherit defaults. `Some(empty)` = clear defaults.
/// `Some(non-empty)` = append/override defaults by id (state wins).
fn effective_mcp_entries(
    defaults: &[StateMcpEntry],
    state: Option<&[StateMcpEntry]>,
) -> Vec<StateMcpEntry> {
    match state {
        None => defaults.to_vec(),
        Some(list) if list.is_empty() => Vec::new(),
        Some(list) => {
            let mut out: Vec<StateMcpEntry> = defaults.to_vec();
            for entry in list {
                if let Some(pos) = out.iter().position(|e| e.id() == entry.id()) {
                    out[pos] = entry.clone();
                } else {
                    out.push(entry.clone());
                }
            }
            out
        }
    }
}

fn effective_skill_entries(
    defaults: &[StateSkillEntry],
    state: Option<&[StateSkillEntry]>,
) -> Vec<StateSkillEntry> {
    match state {
        None => defaults.to_vec(),
        Some(list) if list.is_empty() => Vec::new(),
        Some(list) => {
            let mut out: Vec<StateSkillEntry> = defaults.to_vec();
            for entry in list {
                if let Some(pos) = out.iter().position(|e| e.id() == entry.id()) {
                    out[pos] = entry.clone();
                } else {
                    out.push(entry.clone());
                }
            }
            out
        }
    }
}

/// Resolve one entry against the registry. Inline definitions on the entry
/// take precedence over registry lookups.
fn resolve_mcp_entry(
    entry: &StateMcpEntry,
    registry: &BTreeMap<String, McpServerProfile>,
) -> ResolvedMcpEntry {
    let id = entry.id().to_string();
    let optional = entry.is_optional();
    let inline = match entry {
        StateMcpEntry::Object(obj) if obj.command.is_some() || obj.url.is_some() => {
            Some(inline_mcp_profile(obj))
        }
        _ => None,
    };
    let definition = inline.or_else(|| registry.get(&id).cloned());
    ResolvedMcpEntry { id, optional, definition }
}

fn resolve_skill_entry(
    entry: &StateSkillEntry,
    registry: &BTreeMap<String, SkillProfile>,
) -> ResolvedSkillEntry {
    let id = entry.id().to_string();
    let optional = entry.is_optional();
    let inline = match entry {
        StateSkillEntry::Object(obj) if obj.path.is_some() => Some(SkillProfile {
            path: obj.path.clone().unwrap_or_default(),
            description: obj.description.clone(),
        }),
        _ => None,
    };
    let definition = inline.or_else(|| registry.get(&id).cloned());
    ResolvedSkillEntry { id, optional, definition }
}

fn inline_mcp_profile(obj: &StateMcpEntryObject) -> McpServerProfile {
    McpServerProfile {
        command: obj.command.clone(),
        url: obj.url.clone(),
        transport: obj.transport.clone(),
        env: obj.env.clone(),
        working_directory: obj.working_directory.clone(),
        startup_timeout: obj.startup_timeout.clone(),
    }
}

/// Resolved agent and model for a specific task invocation.
struct ResolvedAgent {
    /// Agent id (key into the merged `agents` registry).
    agent: AgentConfig,
    /// The registry-resolved transport profile for `agent`.
    profile: CustomAgentProfile,
    /// Resolved mode name, or `None` if the agent has no modes or none was
    /// selected.
    mode: Option<String>,
    model: Option<String>,
    timeout_secs: Option<u64>,
}

#[derive(Clone)]
enum ProgramCommand {
    Shell(String),
    Exec(Vec<String>),
}

#[derive(Clone)]
struct ProgramSpec {
    command: ProgramCommand,
    env: BTreeMap<String, String>,
    working_directory: Option<String>,
    shell: bool,
}

struct ResolvedProgram {
    program: ProgramSpec,
    timeout_secs: Option<u64>,
}

/// Resolve the agent/model/mode/timeout for a task's current state.
///
/// Agent id:  CLI override > state-level > project/global settings.
/// Model:     CLI override > state-level > settings.
/// Mode:      CLI override > state-level `agent_mode` > `defaults.agent_mode`
///            > top-level `agent_mode` > the profile's first declared mode.
/// Timeout:   state `agent_timeout` > profile `timeout` > settings `agent_timeout`.
///
/// The resolved agent id must match an entry in the merged `agents` registry;
/// unknown ids produce a `MietteResult` error.
fn resolve_agent(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
) -> MietteResult<Option<ResolvedAgent>> {
    if opts.no_agent() {
        return Ok(None);
    }

    let state_def = machine.states.get(state_name);

    // Agent id resolution: CLI > state > settings.
    let agent = if let Some(ovr) = opts.agent_override() {
        Some(AgentConfig::from(ovr))
    } else if let Some(a) = state_def.and_then(|d| d.agent.clone()) {
        Some(a)
    } else {
        settings.agent.clone()
    };

    let Some(agent) = agent else {
        return Ok(None);
    };

    // Registry lookup. An id that is not in the merged registry is a
    // configuration error — no silent fallback to treating the id as a raw
    // binary name.
    let profile = settings.agents.get(agent.id()).cloned().ok_or_else(|| {
        miette!(
            "agent '{}' is not defined. Add an entry to agents.<id> in \
             .rhei/settings.json or ~/.config/rhei/settings.json, or \
             reference one of the built-in ids ({}).",
            agent.id(),
            built_in_agents()
                .keys()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    // Model resolution: CLI > state > settings.
    let model = if let Some(ovr) = opts.model_override() {
        Some(ovr.to_string())
    } else if let Some(m) = state_def.and_then(|d| d.model.clone()) {
        Some(m)
    } else {
        settings.model.clone()
    };

    // Mode resolution.
    let mode = if let Some(ovr) = opts.agent_mode_override() {
        Some(ovr.to_string())
    } else if let Some(m) = state_def.and_then(|d| d.agent_mode.clone()) {
        Some(m)
    } else if let Some(m) = settings.defaults.agent_mode.clone() {
        Some(m)
    } else if let Some(m) = settings.agent_mode.clone() {
        Some(m)
    } else {
        profile.modes.keys().next().cloned()
    };

    if let Some(name) = &mode {
        if !profile.modes.contains_key(name) && !profile.modes.is_empty() {
            return Err(miette!(
                "agent '{}' has no mode '{}'. Available modes: {}.",
                agent.id(),
                name,
                profile
                    .modes
                    .keys()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // Timeout resolution: state > profile > settings.
    let timeout_secs = state_def
        .and_then(|d| d.agent_timeout.as_deref())
        .and_then(rhei_validator::parse_duration_secs)
        .or_else(|| profile.timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| {
            settings.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        });

    Ok(Some(ResolvedAgent { agent, profile, mode, model, timeout_secs }))
}

fn parse_program_spec(value: &YamlValue) -> MietteResult<ProgramSpec> {
    match value {
        YamlValue::String(command) => Ok(ProgramSpec {
            command: ProgramCommand::Shell(command.clone()),
            env: BTreeMap::new(),
            working_directory: None,
            shell: true,
        }),
        YamlValue::Mapping(mapping) => {
            let command = mapping
                .get(yaml_key("command"))
                .ok_or_else(|| miette!("program object must include a 'command' field"))?;
            let command = match command {
                YamlValue::String(value) => ProgramCommand::Shell(value.clone()),
                YamlValue::Sequence(items) => ProgramCommand::Exec(
                    items
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .map(str::to_string)
                                .ok_or_else(|| miette!("program.command entries must be strings"))
                        })
                        .collect::<MietteResult<Vec<_>>>()?,
                ),
                _ => return Err(miette!("program.command must be a string or string array")),
            };

            let env = mapping
                .get(yaml_key("env"))
                .map(|value| match value {
                    YamlValue::Mapping(values) => values
                        .iter()
                        .map(|(key, value)| {
                            let key = key
                                .as_str()
                                .ok_or_else(|| miette!("program.env keys must be strings"))?;
                            let value = match value {
                                YamlValue::Null => String::new(),
                                YamlValue::Bool(value) => value.to_string(),
                                YamlValue::Number(value) => value.to_string(),
                                YamlValue::String(value) => value.clone(),
                                _ => {
                                    return Err(miette!(
                                        "program.env values must be strings, numbers, booleans, or null"
                                    ))
                                }
                            };
                            Ok((key.to_string(), value))
                        })
                        .collect::<MietteResult<BTreeMap<_, _>>>(),
                    _ => Err(miette!("program.env must be a mapping")),
                })
                .transpose()?
                .unwrap_or_default();

            let working_directory = mapping
                .get(yaml_key("working_directory"))
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| miette!("program.working_directory must be a string"))
                })
                .transpose()?;

            let shell = mapping
                .get(yaml_key("shell"))
                .and_then(YamlValue::as_bool)
                .unwrap_or(matches!(command, ProgramCommand::Shell(_)));

            Ok(ProgramSpec { command, env, working_directory, shell })
        }
        _ => Err(miette!("program must be a string or object")),
    }
}

fn resolve_program(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
) -> MietteResult<Option<ResolvedProgram>> {
    if opts.no_program() {
        return Ok(None);
    }

    let state_def = machine
        .states
        .get(state_name)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", state_name))?;
    let Some(program_value) = state_def.program.as_ref() else {
        return Ok(None);
    };

    let timeout_secs = opts
        .program_timeout_override()
        .and_then(rhei_validator::parse_duration_secs)
        .or_else(|| {
            state_def.program_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        })
        .or_else(|| {
            settings.program_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        });

    Ok(Some(ResolvedProgram { program: parse_program_spec(program_value)?, timeout_secs }))
}

/// Compose the prompt that will be sent to the agent.
fn compose_agent_prompt(render_context: &RuntimeTemplateContext<'_>) -> String {
    let state_def = render_context.machine.states.get(render_context.state_name);
    let instructions = resolve_runtime_template_text(
        state_def.and_then(|d| d.instructions.as_deref()).unwrap_or("").trim(),
        render_context,
    );
    let personality = state_def
        .and_then(|d| d.personality.as_deref())
        .map(str::trim)
        .map(|text| resolve_runtime_template_text(text, render_context));

    // Build available transitions list.
    let mut transitions_list = String::new();
    for rule in &render_context.machine.transitions {
        if rule.from.0 == render_context.state_name || rule.from.0 == "*" {
            transitions_list.push_str(&format!("- {} -> {}", render_context.state_name, rule.to.0));
            if let Some(cond) = &rule.condition {
                transitions_list.push_str(&format!(" (when {})", cond));
            }
            transitions_list.push('\n');
        }
    }

    let plan_path_str = render_context.plan_path.display().to_string();
    let rhei_cli = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "rhei".to_string());
    let state_machine_arg = render_context
        .state_machine_path
        .map(|path| format!(" --state-machine {}", path.display()))
        .unwrap_or_default();
    let state_machine_label = render_context
        .state_machine_path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "the built-in default".to_string());
    let task_id = render_context.task.id.to_string();

    let mut prompt = format!(
        "# Task {task_id}: {}\n\n## State: {}\n",
        render_context.task.title, render_context.state_name
    );
    if let Some(p) = personality {
        prompt.push_str(&format!("\n{p}\n"));
    }
    prompt.push_str(&format!("\n## Instructions\n\n{instructions}\n"));
    if !render_context.task.content.trim().is_empty() {
        prompt.push_str(&format!("\n## Task Content\n\n{}\n", render_context.task.content.trim()));
    }
    if !render_context.task.subtasks.is_empty() {
        prompt.push_str("\n## Subtasks\n\n");
        for sub in &render_context.task.subtasks {
            prompt.push_str(&format!(
                "- Subtask {}.{}: {} [{}]\n",
                sub.task_number, sub.subtask_number, sub.title, sub.state
            ));
        }
    }
    prompt.push_str(&format!(
        "\n## Rhei Commands\n\n\
         You are working in a rhei-managed plan at `{plan_path_str}`.\n\
         The active state machine is `{state_machine_label}`.\n\
         Use these commands to advance the task:\n\n\
         - `{rhei_cli}{state_machine_arg} transition {plan_path_str} --task {task_id} --from {} --to <target>` \
         -- advance to the next state\n\
         - `{rhei_cli}{state_machine_arg} complete {plan_path_str} --task {task_id} --result \"<message>\"` \
         -- complete the task\n\n\
         Before you stop, create every required output artifact for this state and then run exactly one of the commands above.\n\
         Do not leave the task in its current state after writing files, and do not continue into speculative future passes once the current state's artifact and transition are done.\n\n\
         Available transitions from `{}`:\n{transitions_list}\n\
         Do not modify **State:** lines in the plan directly. Use the rhei CLI.\n",
        render_context.state_name, render_context.state_name
    ));
    prompt
}

/// Build a `Command` for the resolved agent.
///
/// Flag order:
/// `<command...> <mode flags...> <prompt_flag> <prompt>? <model_flag> <model>?`
/// `-- ` is appended after the model flag when `stdin_prompt` is `true`, to
/// match `codex exec -- `-style invocations that expect stdin.
#[allow(clippy::too_many_arguments)]
fn build_agent_command(
    resolved: &ResolvedAgent,
    prompt: &str,
    working_dir: &Path,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    task_id: &str,
    state_name: &str,
    tooling: &ResolvedTooling,
) -> std::process::Command {
    let profile = &resolved.profile;
    let id = resolved.agent.id();

    let (program, base_args) =
        profile.command.split_first().expect("registry profile has non-empty command");

    let mut cmd = std::process::Command::new(program);
    cmd.current_dir(working_dir);
    for arg in base_args {
        cmd.arg(arg);
    }

    if let Some(mode) = resolved.mode.as_deref() {
        if let Some(flags) = profile.modes.get(mode) {
            for arg in flags {
                cmd.arg(arg);
            }
        }
    }

    if profile.stdin_prompt {
        cmd.stdin(std::process::Stdio::piped());
    } else if let Some(flag) = &profile.prompt_flag {
        cmd.arg(flag).arg(prompt);
    }

    if let (Some(flag), Some(model)) = (&profile.model_flag, &resolved.model) {
        cmd.arg(flag).arg(model);
    }

    if profile.stdin_prompt {
        cmd.arg("--");
    }

    cmd.env("RHEI_PLAN_PATH", plan_path)
        .env("RHEI_TASK_ID", task_id)
        .env("RHEI_STATE", state_name)
        .env("RHEI_AGENT", id);
    if let Some(path) = state_machine_path {
        cmd.env("RHEI_STATE_MACHINE_PATH", path);
    }
    if let Some(model) = &resolved.model {
        cmd.env("RHEI_MODEL", model);
    }
    if let Some(mode) = &resolved.mode {
        cmd.env("RHEI_AGENT_MODE", mode);
    }
    inject_tooling_env(&mut cmd, tooling);
    cmd
}

/// Format the `mcp_servers:` / `skills:` line in the agent log header.
///
/// Returns `None` when the slice is empty (no line is written). An entry
/// suffixed with `?` is `optional: true` and failed its availability check —
/// it was dropped before spawn but appears here for diagnostics.
fn format_tooling_log_line<T, F>(entries: &[T], project: F) -> Option<String>
where
    F: Fn(&T) -> (&str, bool, bool),
{
    if entries.is_empty() {
        return None;
    }
    let rendered: Vec<String> = entries
        .iter()
        .map(|entry| {
            let (id, optional, available) = project(entry);
            if optional && !available {
                format!("{id}?")
            } else {
                id.to_string()
            }
        })
        .collect();
    Some(rendered.join(","))
}

/// Set `RHEI_MCP_*` and `RHEI_SKILL_*` env vars on the agent command.
///
/// Aggregates exposed:
/// - `RHEI_MCP_SERVERS`: comma-separated ids whose registry lookup succeeded
/// - `RHEI_SKILLS`: same, for skills
///
/// Per-entry availability is exposed as `RHEI_MCP_<NAME>_AVAILABLE` and
/// `RHEI_SKILL_<ID>_AVAILABLE` with `<NAME>` / `<ID>` normalized by
/// [`env_id_segment`].
fn inject_tooling_env(cmd: &mut std::process::Command, tooling: &ResolvedTooling) {
    cmd.env("RHEI_MCP_SERVERS", tooling.mcp_servers_csv());
    cmd.env("RHEI_SKILLS", tooling.skills_csv());
    for entry in &tooling.mcp_servers {
        cmd.env(
            format!("RHEI_MCP_{}_AVAILABLE", env_id_segment(&entry.id)),
            entry.definition.is_some().to_string(),
        );
    }
    for entry in &tooling.skills {
        cmd.env(
            format!("RHEI_SKILL_{}_AVAILABLE", env_id_segment(&entry.id)),
            entry.definition.is_some().to_string(),
        );
    }
}

/// Construct the log file path for a task/state invocation.
fn agent_log_path(runtime_dir: &Path, task_id: &str, state_name: &str) -> PathBuf {
    runtime_dir.join("logs").join(format!("task-{task_id}-{state_name}.log"))
}

/// Spawn an agent, capture output to a log file, and wait with timeout.
///
/// Returns the exit status (or an error on timeout/failure).
#[allow(clippy::too_many_arguments)]
fn spawn_and_wait_agent(
    resolved: &ResolvedAgent,
    prompt: &str,
    working_dir: &Path,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    task_id: &str,
    state_name: &str,
    tooling: &ResolvedTooling,
    log_path: &Path,
) -> MietteResult<std::process::ExitStatus> {
    // Ensure log directory exists.
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create log directory '{}': {e}", parent.display()))?;
    }

    let log_file = fs::File::create(log_path)
        .map_err(|e| miette!("failed to create log file '{}': {e}", log_path.display()))?;

    // Write log header.
    {
        use std::io::Write as _;
        let mut f = &log_file;
        let _ = writeln!(f, "=== rhei agent log ===");
        let _ = writeln!(f, "agent: {}", resolved.agent.id());
        if let Some(mode) = &resolved.mode {
            let _ = writeln!(f, "mode: {mode}");
        }
        if let Some(m) = &resolved.model {
            let _ = writeln!(f, "model: {m}");
        }
        let _ = writeln!(f, "task: {task_id}");
        let _ = writeln!(f, "state: {state_name}");
        if let Some(t) = resolved.timeout_secs {
            let _ = writeln!(f, "timeout: {t}s");
        }
        let _ = writeln!(f, "plan: {}", plan_path.display());
        let mcp_line = format_tooling_log_line(&tooling.mcp_servers, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        if let Some(line) = mcp_line {
            let _ = writeln!(f, "mcp_servers: {line}");
        }
        let skill_line = format_tooling_log_line(&tooling.skills, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        if let Some(line) = skill_line {
            let _ = writeln!(f, "skills: {line}");
        }
        let _ = writeln!(f, "===\n");
    }

    let log_stdout =
        log_file.try_clone().map_err(|e| miette!("failed to clone log file handle: {e}"))?;
    let log_stderr =
        log_file.try_clone().map_err(|e| miette!("failed to clone log file handle: {e}"))?;

    let mut cmd = build_agent_command(
        resolved,
        prompt,
        working_dir,
        plan_path,
        state_machine_path,
        task_id,
        state_name,
        tooling,
    );
    cmd.stdout(log_stdout).stderr(log_stderr);

    let mut child =
        cmd.spawn().map_err(|e| miette!("failed to spawn agent '{}': {e}", resolved.agent.id()))?;

    // If stdin_prompt, write prompt to stdin.
    if resolved.profile.stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write as _;
            let _ = stdin.write_all(prompt.as_bytes());
            drop(stdin);
        }
    }

    let start = Instant::now();

    // Wait with optional timeout.
    let status = if let Some(timeout_secs) = resolved.timeout_secs {
        let timeout = Duration::from_secs(timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        // Send SIGTERM.
                        let pid = Pid::from_raw(child.id() as i32);
                        let _ = signal::kill(pid, Signal::SIGTERM);
                        // Grace period.
                        std::thread::sleep(Duration::from_secs(10));
                        match child.try_wait() {
                            Ok(Some(status)) => break Ok(status),
                            _ => {
                                let _ = child.kill(); // SIGKILL
                                break child.wait().map_err(|e| {
                                    miette!("failed to wait for agent after kill: {e}")
                                });
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => break Err(miette!("error waiting for agent: {e}")),
            }
        }
    } else {
        child.wait().map_err(|e| miette!("failed to wait for agent: {e}"))
    }?;

    // Write log footer.
    {
        use std::io::Write as _;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(log_path)
            .map_err(|e| miette!("failed to append to log file: {e}"))?;
        let _ = writeln!(f, "\n=== exit ===");
        let _ = writeln!(f, "code: {}", status.code().unwrap_or(-1));
        let elapsed = start.elapsed();
        let _ = writeln!(f, "duration: {}s", elapsed.as_secs());
        let _ = writeln!(f, "===");
    }

    Ok(status)
}

fn program_log_path(runtime_dir: &Path, task_id: &str, state_name: &str) -> PathBuf {
    runtime_dir.join("logs").join(format!("task-{task_id}-{state_name}.log"))
}

fn build_program_command(
    resolved: &ResolvedProgram,
    render_context: &RuntimeTemplateContext<'_>,
) -> MietteResult<std::process::Command> {
    let working_dir = resolved
        .program
        .working_directory
        .as_deref()
        .map(|path| resolve_runtime_template_text(path, render_context))
        .map(|path| render_context.workspace_root.join(path))
        .unwrap_or_else(|| render_context.workspace_root.to_path_buf());

    let mut cmd = if resolved.program.shell {
        let mut command = std::process::Command::new("sh");
        command.arg("-c");
        let rendered = match &resolved.program.command {
            ProgramCommand::Shell(command) => {
                resolve_runtime_template_text(command, render_context)
            }
            ProgramCommand::Exec(args) => args
                .iter()
                .map(|arg| resolve_runtime_template_text(arg, render_context))
                .collect::<Vec<_>>()
                .join(" "),
        };
        command.arg(rendered);
        command
    } else {
        let ProgramCommand::Exec(args) = &resolved.program.command else {
            return Err(miette!("program command cannot disable shell when command is a string"));
        };
        let rendered = args
            .iter()
            .map(|arg| resolve_runtime_template_text(arg, render_context))
            .collect::<Vec<_>>();
        let mut command = std::process::Command::new(&rendered[0]);
        for arg in &rendered[1..] {
            command.arg(arg);
        }
        command
    };

    cmd.current_dir(working_dir)
        .env("RHEI_PLAN_PATH", render_context.plan_path)
        .env("RHEI_TASK_ID", render_context.task.id.to_string())
        .env("RHEI_STATE", render_context.state_name)
        .env(
            "RHEI_VISIT_COUNT",
            render_visit_count(
                render_context.metadata,
                &render_context.task.id,
                render_context.state_name,
                render_context.current_state_raw,
                render_context.machine,
            )
            .to_string(),
        );
    if let Some(path) = render_context.state_machine_path {
        cmd.env("RHEI_STATE_MACHINE_PATH", path);
    }
    if let Some(model) = render_context.model {
        cmd.env("RHEI_MODEL", model);
    }

    // Expose declared input artifact paths and existence flags so programs
    // can branch on optional inputs without shelling out to test -f.
    let input_visit_count = Some(render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    ));
    if let Some(state_def) = render_context.machine.states.get(render_context.state_name) {
        for artifact in &state_def.inputs {
            let env_base = artifact.name.to_uppercase().replace(['-', ' '], "_");
            let (relative, path) = resolve_artifact_path(
                render_context.workspace_root,
                artifact,
                &render_context.task.id.to_string(),
                render_context.state_name,
                input_visit_count,
                render_context.model,
            );
            cmd.env(format!("RHEI_INPUT_{env_base}_EXISTS"), path.exists().to_string());
            cmd.env(format!("RHEI_INPUT_{env_base}_PATH"), relative);
        }
    }

    for (key, value) in &resolved.program.env {
        cmd.env(key, resolve_runtime_template_text(value, render_context));
    }

    Ok(cmd)
}

fn spawn_and_wait_program(
    resolved: &ResolvedProgram,
    render_context: &RuntimeTemplateContext<'_>,
    log_path: &Path,
) -> MietteResult<std::process::ExitStatus> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create log directory '{}': {e}", parent.display()))?;
    }

    let log_file = fs::File::create(log_path)
        .map_err(|e| miette!("failed to create log file '{}': {e}", log_path.display()))?;
    {
        use std::io::Write as _;
        let mut f = &log_file;
        let command_label = match &resolved.program.command {
            ProgramCommand::Shell(command) => {
                resolve_runtime_template_text(command, render_context)
            }
            ProgramCommand::Exec(args) => args
                .iter()
                .map(|arg| resolve_runtime_template_text(arg, render_context))
                .collect::<Vec<_>>()
                .join(" "),
        };
        let _ = writeln!(f, "=== rhei program log v1 ===");
        let _ = writeln!(f, "program: {command_label}");
        let _ = writeln!(f, "task: {}", render_context.task.id);
        let _ = writeln!(f, "state: {}", render_context.state_name);
        if let Some(timeout) = resolved.timeout_secs {
            let _ = writeln!(f, "timeout: {timeout}s");
        }
        let _ = writeln!(f, "plan: {}", render_context.plan_path.display());
        let _ = writeln!(f, "===\n");
    }

    let log_stdout =
        log_file.try_clone().map_err(|e| miette!("failed to clone log file handle: {e}"))?;
    let log_stderr =
        log_file.try_clone().map_err(|e| miette!("failed to clone log file handle: {e}"))?;
    let mut cmd = build_program_command(resolved, render_context)?;
    cmd.stdout(log_stdout).stderr(log_stderr);
    let mut child = cmd.spawn().map_err(|e| miette!("failed to spawn program: {e}"))?;
    let start = Instant::now();

    let status = if let Some(timeout_secs) = resolved.timeout_secs {
        let timeout = Duration::from_secs(timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let pid = Pid::from_raw(child.id() as i32);
                        let _ = signal::kill(pid, Signal::SIGTERM);
                        std::thread::sleep(Duration::from_secs(10));
                        match child.try_wait() {
                            Ok(Some(status)) => break Ok(status),
                            _ => {
                                let _ = child.kill();
                                break child.wait().map_err(|e| {
                                    miette!("failed to wait for program after kill: {e}")
                                });
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => break Err(miette!("error waiting for program: {e}")),
            }
        }
    } else {
        child.wait().map_err(|e| miette!("failed to wait for program: {e}"))
    }?;

    {
        use std::io::Write as _;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(log_path)
            .map_err(|e| miette!("failed to append to log file: {e}"))?;
        let _ = writeln!(f, "\n=== exit ===");
        let _ = writeln!(f, "code: {}", status.code().unwrap_or(-1));
        let _ = writeln!(f, "duration: {}s", start.elapsed().as_secs());
        let _ = writeln!(f, "===");
    }

    Ok(status)
}

fn transition_matches_exit_code(rule: &rhei_core::ast::TransitionRule, exit_code: i32) -> bool {
    match rule.exit_code.as_ref() {
        Some(YamlValue::Number(number)) => number.as_i64() == Some(i64::from(exit_code)),
        Some(YamlValue::Sequence(values)) => {
            values.iter().filter_map(YamlValue::as_i64).any(|value| value == i64::from(exit_code))
        }
        Some(YamlValue::String(value)) if value == "nonzero" => exit_code != 0,
        _ => false,
    }
}

fn find_program_exit_transition(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
    exit_code: i32,
) -> MietteResult<Option<String>> {
    let specific_matches = machine
        .transitions
        .iter()
        .filter(|rule| rule.from.0 == current_state)
        .filter(|rule| {
            matches!(rule.exit_code, Some(YamlValue::Number(_)) | Some(YamlValue::Sequence(_)))
        })
        .filter(|rule| transition_matches_exit_code(rule, exit_code))
        .filter(|rule| {
            transition_rule_is_applicable(
                rule,
                machine,
                metadata,
                &task.id,
                current_state,
                task.state.as_str(),
            )
            .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if specific_matches.len() > 1 {
        return Err(miette!(
            "multiple exit_code transitions matched exit code {} from state '{}'",
            exit_code,
            current_state
        ));
    }
    if let Some(rule) = specific_matches.first() {
        return Ok(Some(rule.to.0.clone()));
    }
    if exit_code == 0 {
        return Ok(None);
    }

    let nonzero_matches = machine
        .transitions
        .iter()
        .filter(|rule| rule.from.0 == current_state)
        .filter(|rule| matches!(rule.exit_code, Some(YamlValue::String(ref value)) if value == "nonzero"))
        .filter(|rule| {
            transition_rule_is_applicable(
                rule,
                machine,
                metadata,
                &task.id,
                current_state,
                task.state.as_str(),
            )
            .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if nonzero_matches.len() > 1 {
        return Err(miette!(
            "multiple nonzero exit_code transitions matched exit code {} from state '{}'",
            exit_code,
            current_state
        ));
    }

    Ok(nonzero_matches.first().map(|rule| rule.to.0.clone()))
}

/// Execute the `run` subcommand: advance tasks through the state machine
/// in dependency order.
///
/// In agent mode (the default when an agent is configured), spawns coding
/// agents for each task. In callback-only mode (`--no-agent`), advances
/// tasks through transition callbacks only.
fn run_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    opts: RunOptions,
) -> MietteResult<()> {
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root);

    // Warn if --parallel > 1 on single-file plans.
    let is_workspace = workspace::is_workspace(input);
    let effective_parallel = if opts.parallel() > 1 && !is_workspace {
        eprintln!(
            "warning: --parallel > 1 is not supported for single-file plans (risk of \
             conflicting edits). Falling back to sequential execution."
        );
        1
    } else {
        opts.parallel()
    };

    // Initial validation pass.
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    let initial_terminal_count = loaded
        .rhei
        .tasks
        .iter()
        .filter(|task| is_terminal_state(task.state.as_str(), &machine))
        .count();
    println!(
        "Running {} '{}' with {} task(s) ({} terminal at start).",
        if is_workspace { "workspace" } else { "plan" },
        loaded.rhei.title,
        loaded.rhei.tasks.len(),
        initial_terminal_count
    );
    println!("Initial states: {}", format_state_counts(&loaded.rhei));

    let mut use_standalone_mode = false;
    for (name, def) in &machine.states {
        if def.terminal || def.gating {
            continue;
        }
        if def.program.is_some() {
            use_standalone_mode = true;
            break;
        }
        if resolve_agent(&machine, name, &settings, &opts)?.is_some() {
            use_standalone_mode = true;
            break;
        }
    }

    if use_standalone_mode {
        run_agent_mode(input, &machine, &callback_paths, &settings, &opts, effective_parallel)
    } else {
        run_callback_mode(input, &machine, &callback_paths, &opts)
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
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let runtime_dir = workspace_root.join("runtime");
    let mut agents_spawned = 0u32;
    let mut programs_spawned = 0u32;
    let mut pass = 0u32;

    loop {
        let loaded = load_plan(input)?;
        let ready = find_ready_tasks(&loaded.rhei, machine);
        if ready.is_empty() {
            break;
        }

        pass += 1;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|task| is_terminal_state(task.state.as_str(), machine))
            .count();
        println!(
            "\nPass {}: {} ready, {} terminal, {} total.",
            pass,
            ready.len(),
            terminal_count,
            loaded.rhei.tasks.len()
        );
        println!("Ready: {}", format_ready_tasks(&ready));

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
                println!(
                    "Task {} is in gating state '{}'. Waiting for human action.",
                    task_id_str, current_state
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
            } else if let Some(resolved) = resolve_agent(machine, &current_state, settings, opts)? {
                agent_tasks.push((task_id_str, current_state_raw, current_state, resolved));
            } else if opts.no_agent() {
                callback_tasks.push((task_id_str, current_state_raw, current_state));
            } else {
                return Err(miette!(
                    "no agent configured.\nSet one in ~/.config/rhei/settings.json, .rhei/settings.json, or the state machine.\nAlternatively, pass --agent <AGENT> to rhei run."
                ));
            }
        }

        let mut advanced_any = false;

        // Handle callback-only tasks first (fast, synchronous).
        for (task_id_str, current_state_raw, current_state) in &callback_tasks {
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let task = match loaded.rhei.tasks.iter().find(|t| t.id == target_id) {
                Some(t) => t,
                None => continue,
            };
            let next_to = find_next_transition(task, &loaded.rhei, machine)?;
            let Some(to_state) = next_to else { continue };

            if opts.dry_run() {
                println!(
                    "Would transition Task {} from '{}' to '{}'",
                    task_id_str, current_state_raw, to_state
                );
                continue;
            }

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
                Ok(()) => {
                    println!(
                        "Task {} transitioned: '{}' \u{2192} '{}'",
                        task_id_str, current_state_raw, to_state
                    );
                    advanced_any = true;
                }
                Err(err) => {
                    eprintln!("warning: failed to advance Task {}: {}", task_id_str, err);
                }
            }
        }

        if !program_tasks.is_empty() {
            if opts.dry_run() {
                for (task_id_str, current_state_raw, current_state, resolved) in &program_tasks {
                    let loaded = load_plan(input)?;
                    let target_id = parse_task_id(task_id_str);
                    let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
                    let timeout_str = resolved
                        .timeout_secs
                        .map(|s| format!("{s}s"))
                        .unwrap_or_else(|| "none".to_string());
                    let log = program_log_path(&runtime_dir, task_id_str, current_state);
                    println!("\nWould spawn program");
                    if let Some(t) = task {
                        println!("  {} [{}]", format_task_label(t), current_state_raw);
                    }
                    println!("  Timeout: {timeout_str}");
                    println!("  Log: {}", log.display());
                }
                break;
            }

            for (task_id_str, _current_state_raw, current_state, resolved) in &program_tasks {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
                let Some(task) = task else { continue };
                let render_context = RuntimeTemplateContext {
                    workspace_root: &workspace_root,
                    plan_path: &callback_paths.plan_path,
                    state_machine_path: callback_paths.state_machine_path.as_deref(),
                    plan_title: &plan_title,
                    task,
                    state_name: current_state,
                    current_state_raw: task.state.as_str(),
                    machine,
                    metadata: loaded.rhei.metadata.as_ref(),
                    model: None,
                    agent: None,
                    tooling: None,
                };
                let log = program_log_path(&runtime_dir, task_id_str, current_state);

                println!("\nSpawning program for Task {}: {}", task_id_str, task.title);
                println!("  Log: {}", log.display());

                match spawn_and_wait_program(resolved, &render_context, &log) {
                    Ok(status) => {
                        programs_spawned += 1;
                        let mut reloaded = load_plan(input)?;
                        let task_after = reloaded.rhei.tasks.iter().find(|t| t.id == target_id);
                        let mut state_after =
                            task_after.map(|t| t.state.as_str()).unwrap_or("unknown").to_string();

                        if state_after != *current_state {
                            println!(
                                "  Task {} advanced: '{}' -> '{}'",
                                task_id_str, current_state, state_after
                            );
                            advanced_any = true;
                            continue;
                        }

                        if !status.success() && resolved.timeout_secs.is_some() {
                            fire_timeout_transition(
                                input,
                                machine,
                                callback_paths,
                                task_id_str,
                                current_state,
                                opts.no_callbacks(),
                            );
                            reloaded = load_plan(input)?;
                            state_after = reloaded
                                .rhei
                                .tasks
                                .iter()
                                .find(|t| t.id == target_id)
                                .map(|t| t.state.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            if state_after != *current_state {
                                println!(
                                    "  Task {} advanced: '{}' -> '{}'",
                                    task_id_str, current_state, state_after
                                );
                                advanced_any = true;
                                continue;
                            }
                        }

                        let exit_code = status.code().unwrap_or(-1);
                        if let Some(to_state) = find_program_exit_transition(
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            exit_code,
                        )? {
                            let task_file = loaded.task_file(task_id_str, input);
                            let metadata_file = if workspace::is_workspace(input) {
                                input.join("index.rhei.md")
                            } else {
                                task_file.clone()
                            };
                            execute_transition(
                                TransitionFiles {
                                    task_file: &task_file,
                                    metadata_file: &metadata_file,
                                },
                                callback_paths,
                                machine,
                                task_id_str,
                                current_state,
                                &to_state,
                                opts.no_callbacks(),
                            )?;
                            println!(
                                "  Task {} advanced: '{}' -> '{}'",
                                task_id_str, current_state, to_state
                            );
                            advanced_any = true;
                        } else if status.success() {
                            eprintln!(
                                "  warning: program exited 0 but task {} did not advance from '{}'",
                                task_id_str, current_state
                            );
                        } else {
                            eprintln!(
                                "  error: program exited with code {} for task {}",
                                exit_code, task_id_str
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
                        eprintln!("  error: {}", err);
                        if !opts.continue_on_error() {
                            return Err(err);
                        }
                    }
                }
            }
        }

        if agent_tasks.is_empty() {
            if !advanced_any {
                if opts.dry_run() {
                    break;
                }
                println!("No program, agent, or callback-only tasks could advance.");
                break;
            }
            continue;
        }

        // Determine how many agents to spawn this pass.
        let batch_size =
            if max_parallel == 0 { agent_tasks.len() } else { max_parallel.min(agent_tasks.len()) };
        let batch = &agent_tasks[..batch_size];

        if opts.dry_run() {
            for (task_id_str, current_state_raw, current_state, resolved) in batch {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
                let agent_id = resolved.agent.id();
                let model_str = resolved.model.as_deref().unwrap_or("default");
                let timeout_str = resolved
                    .timeout_secs
                    .map(|s| format!("{s}s"))
                    .unwrap_or_else(|| "none".to_string());
                let log = agent_log_path(&runtime_dir, task_id_str, current_state);
                println!("\nWould spawn: {} (model: {model_str})", agent_id);
                if let Some(t) = task {
                    println!("  {} [{}]", format_task_label(t), current_state_raw);
                }
                println!("  Agent: {agent_id}, Model: {model_str}, Timeout: {timeout_str}");
                println!("  Log: {}", log.display());
            }
            break;
        }

        // Spawn agents (sequential or parallel).
        if batch_size == 1 {
            // Sequential: spawn one agent at a time.
            let (task_id_str, _current_state_raw, current_state, resolved) = &batch[0];
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
            let Some(task) = task else { continue };

            let tooling = resolve_tooling(machine, current_state, &settings);
            let render_context = RuntimeTemplateContext {
                workspace_root: &workspace_root,
                plan_path: &callback_paths.plan_path,
                state_machine_path: callback_paths.state_machine_path.as_deref(),
                plan_title: &loaded.rhei.title,
                task,
                state_name: current_state,
                current_state_raw: task.state.as_str(),
                machine,
                metadata: loaded.rhei.metadata.as_ref(),
                model: resolved.model.as_deref(),
                agent: Some(resolved.agent.id()),
                tooling: Some(&tooling),
            };
            let prompt = compose_agent_prompt(&render_context);
            let log = agent_log_path(&runtime_dir, task_id_str, current_state);

            println!(
                "\nSpawning agent '{}' for Task {}: {}",
                resolved.agent.id(),
                task_id_str,
                task.title
            );
            if let Some(m) = &resolved.model {
                println!("  Model: {m}");
            }
            println!("  Log: {}", log.display());

            match spawn_and_wait_agent(
                resolved,
                &prompt,
                &execution_workspace_root(&callback_paths.plan_path),
                &callback_paths.plan_path,
                callback_paths.state_machine_path.as_deref(),
                task_id_str,
                current_state,
                &tooling,
                &log,
            ) {
                Ok(status) => {
                    agents_spawned += 1;
                    let reloaded = load_plan(input)?;
                    let task_after = reloaded.rhei.tasks.iter().find(|t| t.id == target_id);
                    let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
                    let state_before = current_state.as_str();

                    if state_after != state_before {
                        println!(
                            "  Task {} advanced: '{}' -> '{}'",
                            task_id_str, state_before, state_after
                        );
                        advanced_any = true;

                        // Check for timeout transition if agent was killed.
                        if !status.success() && resolved.timeout_secs.is_some() {
                            // Agent may have timed out — check if state changed already
                            // (agent may have called rhei transition before being killed).
                            if state_after == state_before {
                                fire_timeout_transition(
                                    input,
                                    machine,
                                    callback_paths,
                                    task_id_str,
                                    state_before,
                                    opts.no_callbacks(),
                                );
                            }
                        }
                    } else if status.success() {
                        match try_auto_advance_task(
                            input,
                            machine,
                            callback_paths,
                            task_id_str,
                            state_before,
                            opts.no_callbacks(),
                        ) {
                            Ok(Some(to_state)) => {
                                println!(
                                    "  Task {} auto-advanced: '{}' -> '{}'",
                                    task_id_str, state_before, to_state
                                );
                                advanced_any = true;
                            }
                            Ok(None) => {
                                eprintln!(
                                    "  warning: agent exited 0 but task {} did not advance from '{}'",
                                    task_id_str, state_before
                                );
                            }
                            Err(err) => {
                                eprintln!(
                                    "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                                    task_id_str, state_before, err
                                );
                            }
                        }
                    } else {
                        let code = status.code().unwrap_or(-1);
                        eprintln!(
                            "  error: agent exited with code {} for task {}",
                            code, task_id_str
                        );
                        // Check for timeout transition.
                        if resolved.timeout_secs.is_some() {
                            fire_timeout_transition(
                                input,
                                machine,
                                callback_paths,
                                task_id_str,
                                state_before,
                                opts.no_callbacks(),
                            );
                        }
                        if !opts.continue_on_error() {
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
                    eprintln!("  error: {}", err);
                    if !opts.continue_on_error() {
                        return Err(err);
                    }
                }
            }
        } else {
            // Parallel: spawn multiple agents using threads.
            let mut handles = Vec::new();

            for (task_id_str, _current_state_raw, current_state, resolved) in batch {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                let task = loaded.rhei.tasks.iter().find(|t| t.id == target_id);
                let Some(task) = task else { continue };

                let tooling = resolve_tooling(machine, current_state, &settings);
                let render_context = RuntimeTemplateContext {
                    workspace_root: &workspace_root,
                    plan_path: &callback_paths.plan_path,
                    state_machine_path: callback_paths.state_machine_path.as_deref(),
                    plan_title: &loaded.rhei.title,
                    task,
                    state_name: current_state,
                    current_state_raw: task.state.as_str(),
                    machine,
                    metadata: loaded.rhei.metadata.as_ref(),
                    model: resolved.model.as_deref(),
                    agent: Some(resolved.agent.id()),
                    tooling: Some(&tooling),
                };
                let prompt = compose_agent_prompt(&render_context);
                let log = agent_log_path(&runtime_dir, task_id_str, current_state);
                let working_dir = execution_workspace_root(&callback_paths.plan_path);
                let plan_path = callback_paths.plan_path.clone();
                let state_machine_path = callback_paths.state_machine_path.clone();
                let tid = task_id_str.clone();
                let sname = current_state.clone();

                println!(
                    "\nSpawning agent '{}' for Task {}: {} (parallel)",
                    resolved.agent.id(),
                    task_id_str,
                    task.title
                );
                println!("  Log: {}", log.display());

                // Clone what we need for the thread.
                let agent_cfg = resolved.agent.clone();
                let profile_cfg = resolved.profile.clone();
                let mode_cfg = resolved.mode.clone();
                let model_cfg = resolved.model.clone();
                let timeout_cfg = resolved.timeout_secs;
                let tooling_for_thread = tooling.clone();

                let handle = std::thread::spawn(move || {
                    let resolved = ResolvedAgent {
                        agent: agent_cfg,
                        profile: profile_cfg,
                        mode: mode_cfg,
                        model: model_cfg,
                        timeout_secs: timeout_cfg,
                    };
                    let result = spawn_and_wait_agent(
                        &resolved,
                        &prompt,
                        &working_dir,
                        &plan_path,
                        state_machine_path.as_deref(),
                        &tid,
                        &sname,
                        &tooling_for_thread,
                        &log,
                    );
                    (tid, sname, result)
                });
                handles.push(handle);
            }

            // Collect results.
            for handle in handles {
                let (task_id_str, state_name, result) = handle.join().unwrap_or_else(|_| {
                    ("?".to_string(), "?".to_string(), Err(miette!("agent thread panicked")))
                });
                match result {
                    Ok(status) => {
                        agents_spawned += 1;
                        let target_id = parse_task_id(&task_id_str);
                        let reloaded = load_plan(input)?;
                        let task_after = reloaded.rhei.tasks.iter().find(|t| t.id == target_id);
                        let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
                        if state_after != state_name {
                            println!(
                                "  Task {} advanced: '{}' -> '{}'",
                                task_id_str, state_name, state_after
                            );
                            advanced_any = true;
                        } else if status.success() {
                            match try_auto_advance_task(
                                input,
                                machine,
                                callback_paths,
                                &task_id_str,
                                &state_name,
                                opts.no_callbacks(),
                            ) {
                                Ok(Some(to_state)) => {
                                    println!(
                                        "  Task {} auto-advanced: '{}' -> '{}'",
                                        task_id_str, state_name, to_state
                                    );
                                    advanced_any = true;
                                }
                                Ok(None) => {
                                    eprintln!(
                                        "  warning: agent exited 0 but task {} did not advance from '{}'",
                                        task_id_str, state_name
                                    );
                                }
                                Err(err) => {
                                    eprintln!(
                                        "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                                        task_id_str, state_name, err
                                    );
                                }
                            }
                        } else {
                            let code = status.code().unwrap_or(-1);
                            eprintln!(
                                "  error: agent exited with code {} for task {}",
                                code, task_id_str
                            );
                            if !opts.continue_on_error() {
                                return Err(miette!(
                                    "agent exited with code {code} for Task {task_id_str}. \
                                     Use --continue-on-error to skip failures."
                                ));
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("  error for task {}: {}", task_id_str, err);
                        if !opts.continue_on_error() {
                            return Err(err);
                        }
                    }
                }
            }
        }

        if !advanced_any {
            break;
        }
    }

    // Print summary.
    if opts.dry_run() {
        println!("\nDry run complete — no programs or agents were spawned.");
    } else if agents_spawned == 0 && programs_spawned == 0 {
        println!("No tasks could be advanced.");
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|t| is_terminal_state(t.state.as_str(), machine))
            .count();
        println!(
            "\nRun complete: {} agent(s), {} program(s) spawned, {}/{} tasks in terminal state.",
            agents_spawned,
            programs_spawned,
            terminal_count,
            loaded.rhei.tasks.len()
        );
        println!("Final states: {}", format_state_counts(&loaded.rhei));
        for task in &loaded.rhei.tasks {
            println!("  - {} [{}]", format_task_label(task), task.state);
        }
    }

    Ok(())
}

/// Callback-only execution mode (legacy behavior, used with --no-agent).
fn run_callback_mode(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    opts: &RunOptions,
) -> MietteResult<()> {
    let mut transitions_made = 0u32;
    let mut pass = 0u32;

    loop {
        let loaded = load_plan(input)?;
        let ready = find_ready_tasks(&loaded.rhei, machine);
        if ready.is_empty() {
            break;
        }

        pass += 1;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|task| is_terminal_state(task.state.as_str(), machine))
            .count();
        println!(
            "\nPass {}: {} ready, {} terminal, {} total.",
            pass,
            ready.len(),
            terminal_count,
            loaded.rhei.tasks.len()
        );
        println!("Ready: {}", format_ready_tasks(&ready));

        let mut advanced_any = false;
        let mut stalled_ready_tasks = Vec::new();

        for task in &ready {
            let task_id_str = task.id.to_string();
            let current_state_raw = task.state.as_str();
            let current_state = normalized_state_name(current_state_raw, machine);
            let next_to = find_next_transition(task, &loaded.rhei, machine)?;

            let Some(to_state) = next_to else {
                stalled_ready_tasks.push(format_task_label(task));
                continue;
            };

            if opts.dry_run() {
                println!(
                    "Would transition Task {} from '{}' to '{}'",
                    task_id_str, current_state_raw, to_state
                );
                continue;
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
                Ok(()) => {
                    println!(
                        "Task {} transitioned: '{}' \u{2192} '{}'",
                        task_id_str, current_state_raw, to_state
                    );
                    println!("  {}", format_task_label(task));
                    if is_terminal_state(&to_state, machine) {
                        println!("  Result: reached terminal state '{}'.", to_state);
                    } else {
                        println!("  Result: now in '{}'.", to_state);
                    }
                    let reloaded = load_plan(input)?;
                    let discovered = newly_discovered_tasks(&task_ids_before, &reloaded.rhei.tasks);
                    if !discovered.is_empty() {
                        println!(
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
                    eprintln!("warning: failed to advance Task {}: {}", task_id_str, err);
                    continue;
                }
            }
        }

        if !stalled_ready_tasks.is_empty() && !advanced_any {
            println!(
                "No forward transition available for ready task(s): {}",
                stalled_ready_tasks.join(", ")
            );
        }

        if opts.dry_run() || !advanced_any {
            break;
        }
    }

    if opts.dry_run() {
        println!("\nDry run complete — no changes were made.");
    } else if transitions_made == 0 {
        println!("No tasks could be advanced.");
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = loaded
            .rhei
            .tasks
            .iter()
            .filter(|t| is_terminal_state(t.state.as_str(), machine))
            .count();
        println!(
            "\nRun complete: {} transition(s) made, {}/{} tasks in terminal state.",
            transitions_made,
            terminal_count,
            loaded.rhei.tasks.len()
        );
        println!("Final states: {}", format_state_counts(&loaded.rhei));
        for task in &loaded.rhei.tasks {
            println!("  - {} [{}]", format_task_label(task), task.state);
        }
    }

    Ok(())
}

/// Try to fire a timeout transition for a task after an agent was killed.
fn fire_timeout_transition(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from_state: &str,
    no_callbacks: bool,
) {
    // Look for a transition with a timeout field from the current state.
    let timeout_rule = machine
        .transitions
        .iter()
        .find(|rule| (rule.from.0 == from_state || rule.from.0 == "*") && rule.timeout.is_some());
    if let Some(rule) = timeout_rule {
        let to_state = &rule.to.0;
        let loaded = match load_plan(input) {
            Ok(l) => l,
            Err(_) => return,
        };
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
            from_state,
            to_state,
            no_callbacks,
        ) {
            Ok(()) => {
                println!(
                    "  Timeout transition: Task {} '{}' -> '{}'",
                    task_id_str, from_state, to_state
                );
            }
            Err(err) => {
                eprintln!(
                    "  warning: failed to fire timeout transition for Task {}: {}",
                    task_id_str, err
                );
            }
        }
    }
}

fn format_task_label(task: &rhei_core::ast::Task) -> String {
    format!("Task {}: {}", task.id, task.title)
}

fn format_ready_tasks(tasks: &[&rhei_core::ast::Task]) -> String {
    tasks.iter().map(|task| format_task_label(task)).collect::<Vec<_>>().join(", ")
}

fn format_state_counts(rhei: &rhei_core::ast::Rhei) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    for task in &rhei.tasks {
        *counts.entry(task.state.as_str()).or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(state, count)| format!("{state}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn newly_discovered_tasks(
    task_ids_before: &BTreeSet<String>,
    tasks_after: &[rhei_core::ast::Task],
) -> Vec<String> {
    tasks_after
        .iter()
        .filter(|task| !task_ids_before.contains(&task.id.to_string()))
        .map(format_task_label)
        .collect()
}

/// Check whether a dependency state satisfies a prerequisite edge.
///
/// Terminal cancellation does not satisfy dependencies: a cancelled task should
/// not unblock downstream work.
fn dependency_is_satisfied(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    normalized_state_name(state, machine) != "cancelled" && is_terminal_state(state, machine)
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
    let state_map: HashMap<&TaskId, String> = rhei
        .tasks
        .iter()
        .map(|t| (&t.id, normalized_state_name(t.state.as_str(), machine)))
        .collect();

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
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).map(|def| def.initial).unwrap_or(false)
        })
        .collect()
}

/// Check whether a state is terminal (final) in the state machine.
fn is_terminal_state(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    let normalized = normalized_state_name(state, machine);
    machine.states.get(&normalized).map(|def| def.terminal).unwrap_or(false)
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
    let current_state = normalized_state_name(task.state.as_str(), machine);

    // First, look for an exact from-state match.
    for rule in machine.transitions() {
        if rule.from.0 == current_state
            && transition_rule_is_applicable(
                rule,
                machine,
                rhei.metadata.as_ref(),
                &task.id,
                &current_state,
                task.state.as_str(),
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
                    &current_state,
                    task.state.as_str(),
                )?
            {
                return Ok(Some(rule.to.0.clone()));
            }
        }
    }

    Ok(None)
}

fn try_auto_advance_task(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    current_state: &str,
    no_callbacks: bool,
) -> MietteResult<Option<String>> {
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(task_id_str);
    let Some(task) = loaded.rhei.tasks.iter().find(|t| t.id == target_id) else {
        return Ok(None);
    };
    let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? else {
        return Ok(None);
    };

    let task_file = loaded.task_file(task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };

    execute_transition(
        TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
        callback_paths,
        machine,
        task_id_str,
        current_state,
        &to_state,
        no_callbacks,
    )?;

    Ok(Some(to_state))
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
            let formatted = format!("**State:** {}", format_state_metadata_value(new_state));
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
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    // Find the target task to claim.
    let (task_id_str, current_state_raw, current_state) = if let Some(tid) = task_id_filter {
        let target_id = parse_task_id(tid);
        let task = loaded
            .rhei
            .tasks
            .iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| miette!("task '{}' not found in the plan", tid))?;
        let state_name = normalized_state_name(task.state.as_str(), &machine);
        let is_initial = machine.states.get(&state_name).map(|def| def.initial).unwrap_or(false);
        if is_initial {
            let state_map: HashMap<&TaskId, String> = loaded
                .rhei
                .tasks
                .iter()
                .map(|t| (&t.id, normalized_state_name(t.state.as_str(), &machine)))
                .collect();
            let all_priors_done = task.prior.iter().all(|dep_id| {
                state_map.get(dep_id).map(|s| dependency_is_satisfied(s, &machine)).unwrap_or(false)
            });
            if !all_priors_done {
                return Err(miette!("Task {} is blocked by incomplete prerequisites", tid));
            }
        }
        let state_def = machine
            .states
            .get(&state_name)
            .ok_or_else(|| miette!("state '{}' missing from loaded machine", state_name))?;
        ensure_state_inputs_exist(
            &workspace_root,
            tid,
            &state_name,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state_name,
                task.state.as_str(),
                &machine,
            )),
            None,
            &format!("Task {} cannot be claimed in state {}.", tid, state_name),
        )?;
        (tid.to_string(), task.state.as_str().to_string(), state_name)
    } else {
        let ready = find_claimable_tasks(&loaded.rhei, &machine);
        if ready.is_empty() {
            return Err(miette!("no tasks are ready to claim"));
        }
        let task = ready.into_iter().next().unwrap();
        let state_name = normalized_state_name(task.state.as_str(), &machine);
        let state_def = machine
            .states
            .get(&state_name)
            .ok_or_else(|| miette!("state '{}' missing from loaded machine", state_name))?;
        ensure_state_inputs_exist(
            &workspace_root,
            &task.id.to_string(),
            &state_name,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state_name,
                task.state.as_str(),
                &machine,
            )),
            None,
            &format!("Task {} cannot be claimed in state {}.", task.id, state_name),
        )?;
        (task.id.to_string(), task.state.to_string(), state_name)
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
            miette!("no forward transition available from state '{}'", current_state_raw)
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

    // Resolve agent/model for display.
    let settings = load_merged_settings(&workspace_root);
    let no_agent_opts = RunOptions {
        standalone: StandaloneExecutionFlags {
            dry_run: false,
            no_callbacks: false,
            continue_on_error: false,
            parallel: 1,
        },
        agent: AgentExecutionFlags {
            no_agent: false,
            agent: None,
            agent_mode: None,
            model: None,
        },
        program: ProgramExecutionFlags::default(),
    };
    let resolved = resolve_agent(&machine, &final_state, &settings, &no_agent_opts)?;
    let agent_id_str = resolved.as_ref().map(|r| r.agent.id().to_string());
    let model_id_str = resolved.as_ref().and_then(|r| r.model.clone());
    let tooling = resolve_tooling(&machine, &final_state, &settings);
    let render_context = RuntimeTemplateContext {
        workspace_root: &workspace_root,
        plan_path: &callback_paths.plan_path,
        state_machine_path: callback_paths.state_machine_path.as_deref(),
        plan_title: &loaded.rhei.title,
        task,
        state_name: &final_state,
        current_state_raw: task.state.as_str(),
        machine: &machine,
        metadata: loaded.rhei.metadata.as_ref(),
        model: model_id_str.as_deref(),
        agent: agent_id_str.as_deref(),
        tooling: Some(&tooling),
    };
    let instructions = resolve_runtime_template_text(
        state_instructions(&machine, &final_state).as_str(),
        &render_context,
    );
    let personality = machine
        .states
        .get(final_state.as_str())
        .and_then(|def| def.personality.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|text| resolve_runtime_template_text(text, &render_context));

    print_next_output(NextOutput {
        as_json,
        task,
        from_state: &current_state_raw,
        to_state: task.state.as_str(),
        personality: personality.as_deref(),
        instructions: &instructions,
        agent_id: agent_id_str.as_deref(),
        model_id: model_id_str.as_deref(),
    });

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
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    // Find the task and its current state.
    let target_id = parse_task_id(task_id_str);
    let task = loaded
        .rhei
        .tasks
        .iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
    let current_state_raw = task.state.as_str();
    let current_state = normalized_state_name(current_state_raw, &machine);

    // Reject tasks already in a terminal state.
    if is_terminal_state(current_state_raw, &machine) {
        return Err(miette!(
            "Task {} is already in terminal state '{}'",
            task_id_str,
            current_state_raw
        ));
    }

    let open_subtasks = non_terminal_subtasks(task, &machine);
    if !open_subtasks.is_empty() {
        return Err(miette!(
            "Task {} cannot be completed while subtasks remain non-terminal.\nOffending subtasks: {}",
            task_id_str,
            open_subtasks.join(", ")
        ));
    }

    // Find the completion target: a non-cancelled terminal state reachable via
    // a single declared transition from the current state.
    let to_state = find_completion_state(&current_state, &machine).ok_or_else(|| {
        miette!(
            "no transition to a terminal state available from '{}' for Task {}",
            current_state_raw,
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
        &current_state,
        &to_state,
        no_callbacks,
    )?;

    // Append the completion entry to the result file.
    let root = result_workspace_root(input, &task_file);
    let result_link = format!("runtime/results/{}.md", task_id_str);
    let result_file_existed = root.join(&result_link).exists();
    append_result_entry(&root, task_id_str, current_state_raw, &to_state, Some(result_msg))?;

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
        task_id_str, current_state_raw, to_state, result_link
    );

    Ok(())
}

/// Execute the `reset` subcommand: restore every task and subtask to the
/// state machine's initial state.
///
/// For directory workspaces, this also removes the generated `runtime/`
/// directory so logs and artifacts do not survive the reset.
fn reset_command(input: &Path, state_machine_path: Option<&Path>) -> MietteResult<()> {
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let initial_state = initial_state_name(&resolved.machine)?;

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
            if let Some(metadata) = clear_runtime_state_visits(rhei.metadata.as_ref()) {
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

    let new_raw = if let Some(metadata) = clear_runtime_state_visits(metadata.as_ref()) {
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
            let formatted = format!("**State:** {}", format_state_metadata_value(initial_state));
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

fn non_terminal_subtasks(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> Vec<String> {
    task.subtasks
        .iter()
        .filter(|subtask| !is_terminal_state(subtask.state.as_str(), machine))
        .map(|subtask| {
            format!(
                "Subtask {}.{} ('{}') [{}]",
                subtask.task_number, subtask.subtask_number, subtask.title, subtask.state
            )
        })
        .collect()
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

struct NextOutput<'a> {
    as_json: bool,
    task: &'a rhei_core::ast::Task,
    from_state: &'a str,
    to_state: &'a str,
    personality: Option<&'a str>,
    instructions: &'a str,
    agent_id: Option<&'a str>,
    model_id: Option<&'a str>,
}

/// Print the `next` command output in either human-readable or JSON format.
fn print_next_output(output: NextOutput<'_>) {
    if output.as_json {
        let subtasks: Vec<serde_json::Value> = output
            .task
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

        let mut obj = serde_json::json!({
            "task_id": output.task.id.to_string(),
            "title": output.task.title,
            "from_state": output.from_state,
            "state": output.to_state,
            "personality": output.personality,
            "instructions": output.instructions,
            "content": output.task.content.trim(),
            "subtasks": subtasks,
        });
        if let Some(agent) = output.agent_id {
            obj["agent"] = serde_json::json!(agent);
        }
        if let Some(model) = output.model_id {
            obj["model"] = serde_json::json!(model);
        }
        println!("{}", serde_json::to_string_pretty(&obj).expect("JSON serialization"));
    } else {
        let transitioned = output.from_state != output.to_state;
        if transitioned {
            println!(
                "Task {} claimed: '{}' -> '{}'",
                output.task.id, output.from_state, output.to_state
            );
        } else {
            println!("Task {} (already in '{}')", output.task.id, output.to_state);
        }
        if output.agent_id.is_some() || output.model_id.is_some() {
            let agent_str = output.agent_id.unwrap_or("none");
            let model_str = output.model_id.unwrap_or("default");
            println!("Agent: {} ({})", agent_str, model_str);
        }
        if let Some(personality) = output.personality {
            println!();
            println!("Personality: {}", personality);
        }
        println!();
        println!("## Task {}: {}", output.task.id, output.task.title);
        if !output.task.content.trim().is_empty() {
            println!();
            println!("{}", output.task.content.trim());
        }
        if !output.task.subtasks.is_empty() {
            println!();
            for st in &output.task.subtasks {
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
        if !output.instructions.is_empty() {
            println!();
            println!("--- Instructions ({}) ---", output.to_state);
            println!("{}", output.instructions);
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

/// Install skills for a single agent.
fn install_agent(
    agent: &Agent,
    local: bool,
    link: bool,
    dry_run: bool,
    skill_sources: &[(String, PathBuf)],
    project_root: Option<&Path>,
) -> MietteResult<()> {
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

/// Install skills for Codex.
fn install_codex(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".agents")
    } else {
        home_dir()?.join(".agents")
    };

    let skills_dir = base.join("skills");

    // Install each skill directory. Codex discovers skills by scanning `.agents/skills`
    // (repo-local) and `$HOME/.agents/skills` (user-level).
    for (name, source) in skill_sources {
        if link {
            let dest = skills_dir.join(name);
            let src =
                if local { relative_path(dest.parent().unwrap(), source) } else { source.clone() };
            link_skill(&src, &dest, dry_run)?;
        } else {
            let dest = skills_dir.join(name);
            copy_skill(source, &dest, dry_run)?;
        }
    }

    Ok(())
}

/// Build the content for marker-injected agents (Windsurf, Copilot).
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
                    .join(".agents")
            } else {
                home_dir()?.join(".agents")
            };

            for skill in skills {
                let dest = base.join("skills").join(skill);
                remove_path(&dest, dry_run)?;
            }
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
version: 1
models:
  - gpt-5
  - claude-sonnet
states:
  draft:
    description: planning
    instructions: Wait until author promotes task.
    personality: Ask one sharp planning question first.
    initial: true
    visits: 3
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
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("draft [initial]"));
        assert!(rendered.contains("Visits: 3"));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("Personality: Ask one sharp planning question first."));
        assert!(rendered.contains("Wait until author promotes task."));
        assert!(rendered.contains("done [final]"));
        assert!(rendered.contains("Model: gpt-5"));
        assert!(rendered.contains("draft -> done (on_enter=cli:record_done)"));
    }

    #[test]
    fn render_state_machine_json_includes_state_personality() {
        let yaml = r#"
name: demo
version: 1
models:
  - gpt-5
states:
  draft:
    description: planning
    personality: Focus on planning risks.
    visits: 2
    all_models:
      - gpt-5
    initial: true
transitions: []
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_json(&machine).expect("render JSON");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("parse JSON");

        assert_eq!(json["name"], "demo");
        assert_eq!(json["models"], serde_json::json!(["gpt-5"]));
        assert_eq!(json["states"][0]["personality"], "Focus on planning risks.");
        assert_eq!(json["states"][0]["visits"], 2);
        assert_eq!(json["states"][0]["all_models"], serde_json::json!(["gpt-5"]));
    }

    #[test]
    fn parses_run_command_with_separated_flag_groups() {
        let cli = Cli::try_parse_from([
            "rhei",
            "run",
            "plan.rhei.md",
            "--dry-run",
            "--no-callbacks",
            "--continue-on-error",
            "--parallel",
            "4",
            "--no-agent",
            "--agent",
            "codex",
            "--model",
            "o3",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Run { input, standalone, agent, program } => {
                assert_eq!(input, PathBuf::from("plan.rhei.md"));
                assert!(standalone.dry_run);
                assert!(standalone.no_callbacks);
                assert!(standalone.continue_on_error);
                assert_eq!(standalone.parallel, 4);
                assert!(agent.no_agent);
                assert_eq!(agent.agent.as_deref(), Some("codex"));
                assert_eq!(agent.model.as_deref(), Some("o3"));
                assert!(!program.no_program);
                assert_eq!(program.program_timeout.as_deref(), None);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_help_separates_standalone_and_agent_flags() {
        let mut command = Cli::command();
        let run = command.find_subcommand_mut("run").expect("run subcommand should exist");
        let mut buffer = Vec::new();
        run.write_long_help(&mut buffer).expect("help should render");
        let help = String::from_utf8(buffer).expect("help should be UTF-8");

        assert!(help.contains("Standalone Execution:"));
        assert!(help.contains("--dry-run"));
        assert!(help.contains("--parallel"));
        assert!(help.contains("Agent Execution:"));
        assert!(help.contains("--no-agent"));
        assert!(help.contains("--agent <AGENT>"));
        assert!(help.contains("--model <MODEL>"));
        assert!(help.contains("Program Execution:"));
        assert!(help.contains("--no-program"));
        assert!(help.contains("--program-timeout <DURATION>"));
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
    fn compose_agent_prompt_includes_state_machine_commands_when_present() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Prompt Smoke

## Tasks

### Task demo: Verify prompt wiring
**State:** review

Write findings and transition the task.
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: prompt-smoke
version: 1
states:
  review:
    description: review
    instructions: Write findings to `{output.review-notes.path}`.
    initial: true
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}.md
  fix:
    description: fix
transitions:
  - from: review
    to: fix
"#,
        )
        .expect("machine should parse");
        let task = &rhei.tasks[0];
        let context = RuntimeTemplateContext {
            workspace_root: Path::new("/tmp/workspace"),
            plan_path: Path::new("/tmp/workspace"),
            state_machine_path: Some(Path::new("/tmp/workspace/states.yaml")),
            plan_title: &rhei.title,
            task,
            state_name: "review",
            current_state_raw: "review",
            machine: &machine,
            metadata: None,
            model: None,
            agent: Some("codex"),
            tooling: None,
        };

        let prompt = compose_agent_prompt(&context);

        assert!(prompt.contains("The active state machine is `/tmp/workspace/states.yaml`."));
        assert!(prompt.contains(
            "--state-machine /tmp/workspace/states.yaml transition /tmp/workspace --task demo --from review --to <target>`"
        ));
        assert!(prompt.contains(
            "--state-machine /tmp/workspace/states.yaml complete /tmp/workspace --task demo --result \"<message>\"`"
        ));
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

    // ---- MCP / skills resolution ----

    fn machine_with_tooling(state_yaml: &str) -> rhei_validator::StateMachine {
        let yaml = format!(
            "name: tooling-test\nversion: 1\nstates:\n{state_yaml}\n  completed:\n    description: done\n    final: true\n"
        );
        rhei_validator::StateMachine::from_yaml_str(&yaml).expect("valid state machine")
    }

    fn settings_with(
        defaults_mcp: Option<Vec<StateMcpEntry>>,
        registry: BTreeMap<String, McpServerProfile>,
    ) -> RheiSettings {
        RheiSettings {
            agent: None,
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults {
                agent_mode: None,
                mcp_servers: defaults_mcp,
                skills: None,
            },
            agents: built_in_agents(),
            mcp_servers: registry,
            skills: BTreeMap::new(),
        }
    }

    #[test]
    fn resolve_tooling_unions_defaults_with_state_overrides_by_id() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: linear
        optional: true
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("linear".to_string(), McpServerProfile::default());
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings = settings_with(
            Some(vec![
                StateMcpEntry::Id("postgres".to_string()),
                StateMcpEntry::Id("linear".to_string()),
            ]),
            registry,
        );

        let tooling = resolve_tooling(&machine, "pending", &settings);
        // postgres from defaults stays first; linear from defaults is replaced
        // by the state-level entry that flips optional to true.
        let ids: Vec<&str> = tooling.mcp_servers.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["postgres", "linear"]);
        let linear = tooling.mcp_servers.iter().find(|e| e.id == "linear").unwrap();
        assert!(linear.optional, "state override should win");
        assert!(linear.definition.is_some(), "registry entry resolves");
    }

    #[test]
    fn resolve_tooling_empty_state_list_clears_defaults() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers: []
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings =
            settings_with(Some(vec![StateMcpEntry::Id("postgres".to_string())]), registry);

        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert!(tooling.mcp_servers.is_empty(), "explicit empty clears defaults");
    }

    #[test]
    fn resolve_tooling_omitted_state_inherits_defaults() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings =
            settings_with(Some(vec![StateMcpEntry::Id("postgres".to_string())]), registry);

        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        assert_eq!(tooling.mcp_servers[0].id, "postgres");
    }

    #[test]
    fn resolve_tooling_inline_definition_does_not_require_registry() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: adhoc
        command: ["mcp-adhoc", "--port", "8080"]
"#,
        );
        let settings = settings_with(None, BTreeMap::new());
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        let entry = &tooling.mcp_servers[0];
        assert_eq!(entry.id, "adhoc");
        assert!(entry.definition.is_some(), "inline definition resolves");
        assert_eq!(entry.definition.as_ref().unwrap().command.as_deref().unwrap()[0], "mcp-adhoc");
    }

    #[test]
    fn resolve_tooling_unknown_id_resolves_to_unavailable() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers: [missing]
"#,
        );
        let settings = settings_with(None, BTreeMap::new());
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        assert!(
            tooling.mcp_servers[0].definition.is_none(),
            "unknown id has no definition (Half B reports it as unavailable)"
        );
        assert!(!tooling.mcp_available("missing"));
    }

    #[test]
    fn env_id_segment_normalizes_id() {
        assert_eq!(env_id_segment("linear"), "LINEAR");
        assert_eq!(env_id_segment("ad-hoc"), "AD_HOC");
        assert_eq!(env_id_segment("foo bar"), "FOO_BAR");
        assert_eq!(env_id_segment("a.b.c"), "A_B_C");
    }

    #[test]
    fn format_tooling_log_line_marks_unavailable_optional_with_question_mark() {
        let entries = vec![
            ResolvedMcpEntry {
                id: "postgres".to_string(),
                optional: false,
                definition: Some(McpServerProfile::default()),
            },
            ResolvedMcpEntry { id: "grafana".to_string(), optional: true, definition: None },
        ];
        let line = format_tooling_log_line(&entries, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        assert_eq!(line.as_deref(), Some("postgres,grafana?"));
    }
}
