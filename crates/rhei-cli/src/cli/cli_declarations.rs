use anyhow::{Context, Result};
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::engine::{
    ArgValueCompleter, CompletionCandidate, PathCompleter, ValueCompleter,
};
use clap_complete::env::{
    Bash as CompletionBash, Elvish as CompletionElvish, EnvCompleter, Fish as CompletionFish,
    Powershell as CompletionPowerShell, Zsh as CompletionZsh,
};
use clap_complete::CompleteEnv;
use fs2::FileExt;
use indexmap::IndexMap;
use miette::{miette, Report, Result as MietteResult};
use minijinja::{Environment as MiniJinjaEnvironment, UndefinedBehavior};
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use rhei_core::ast::{Metadata, TaskId};
use rhei_core::callback::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
use rhei_core::workspace;
use rhei_validator::{
    parse_execution_target, AgentConfig, CustomAgentProfile, ExecutionTarget, McpServerProfile,
    SkillProfile, StateMcpEntry, StateMcpEntryObject, StateSkillEntry,
};
use serde::Deserialize;
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(unix)]
fn terminate_child_gracefully(child: &mut std::process::Child) {
    let pid = Pid::from_raw(child.id() as i32);
    let _ = signal::kill(pid, Signal::SIGTERM);
}

#[cfg(not(unix))]
fn terminate_child_gracefully(child: &mut std::process::Child) {
    let _ = child.kill();
}

/// Command-line driver for the Rhei agent runtime.
#[derive(Parser, Debug)]
#[command(
    name = "rhei",
    author,
    version,
    about = "Run governed agent workflows from Markdown plans",
    long_about = None,
    arg_required_else_help = true,
    help_template = "\
{about}

Usage: {usage}

Inspection:
  validate    Validate a markdown plan against the configured states
  lsp         Start the Rhei language server over stdio
  render      Render a markdown plan into a selected output format
  states      Print the states and allowed transitions for the configured state machine
  list        List tasks in a plan with optional filters

Templates:
  templates   List available templates
  instantiate Instantiate a template into a concrete plan or workspace

Execution:
  transition  Atomically transition a task from one state to another (compare-and-swap)
  run         Execute a plan by advancing tasks through the state machine in dependency order
  cost        Inspect run token and cost accounting artifacts
  snapshot    Inspect, prune, or continue from session snapshots
  next        Transition the next ready task to the next state
  complete    Complete a task: transition to terminal state, write result file,\n              link it from the task, and remove the assignee
  reset       Reset all tasks and subtasks to the initial state; for workspaces,\n              also remove runtime output

Setup:
  install-skills  Install rhei skills into AI coding agent configuration directories
  completions     Generate shell completion scripts

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
        help = "Path to a states YAML file (uses built-in default when omitted)",
        add = ArgValueCompleter::new(complete_yaml_path)
    )]
    state_machine: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

fn cli_command() -> clap::Command {
    Cli::command()
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
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
    },
    /// Start the Rhei language server over stdio
    Lsp,
    /// Render a markdown plan into a selected output format
    Render {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
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
    /// List tasks in a plan with optional filters
    List {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        /// Filter by state (repeatable; comma-separated list also accepted)
        #[arg(
            long,
            value_name = "STATE",
            value_delimiter = ',',
            add = ArgValueCompleter::new(complete_comma_state_name)
        )]
        state: Vec<String>,
        /// Filter by assignee value (exact match)
        #[arg(
            long,
            value_name = "ASSIGNEE",
            conflicts_with = "no_assignee",
            add = ArgValueCompleter::new(complete_assignee)
        )]
        assignee: Option<String>,
        /// Only tasks with no assignee
        #[arg(long, conflicts_with = "assignee")]
        no_assignee: bool,
        /// Filter by node kind (e.g. task, bug, spec)
        #[arg(long, value_name = "KIND", add = ArgValueCompleter::new(complete_node_kind))]
        kind: Option<String>,
        /// Only tasks that list <TASK_ID> in their **Prior:** dependencies
        #[arg(long, value_name = "TASK_ID", add = ArgValueCompleter::new(complete_task_id))]
        has_prior: Option<String>,
        /// Only direct children of <TASK_ID>
        #[arg(
            long,
            value_name = "TASK_ID",
            conflicts_with = "root",
            add = ArgValueCompleter::new(complete_task_id)
        )]
        parent: Option<String>,
        /// Only top-level tasks (no parent)
        #[arg(long, conflicts_with = "parent")]
        root: bool,
        /// Substring match against task title and content (case-insensitive)
        #[arg(long, value_name = "TEXT")]
        contains: Option<String>,
        /// Only tasks whose state is terminal in the resolved state machine
        #[arg(long, conflicts_with = "non_terminal")]
        terminal: bool,
        /// Only tasks whose state is non-terminal
        #[arg(long, conflicts_with = "terminal")]
        non_terminal: bool,
        /// Only tasks whose prior dependencies are satisfied and state is non-terminal/non-gating
        #[arg(long, conflicts_with = "blocked")]
        ready: bool,
        /// Only tasks blocked by unsatisfied prerequisites
        #[arg(long, conflicts_with = "ready")]
        blocked: bool,
        /// Maximum number of tasks to print (0 means no limit)
        #[arg(long, default_value_t = 0, add = ArgValueCompleter::new(complete_limit))]
        limit: usize,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
    },
    /// Atomically transition a task from one state to another (compare-and-swap)
    Transition {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        /// Task identifier (number or name)
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: String,
        /// Expected current state of the task
        #[arg(long, add = ArgValueCompleter::new(complete_transition_from_state))]
        from: String,
        /// Target state to transition to
        #[arg(long, add = ArgValueCompleter::new(complete_transition_to_state))]
        to: String,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Execute a plan by advancing tasks through the state machine in dependency order
    Run {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        #[command(flatten)]
        standalone: StandaloneExecutionFlags,
        #[command(flatten)]
        agent: AgentExecutionFlags,
        #[command(flatten)]
        program: ProgramExecutionFlags,
        #[command(flatten)]
        snapshot: SnapshotExecutionFlags,
    },
    /// Inspect run token and cost accounting artifacts
    Cost {
        /// Path to the markdown plan file (.rhei.md) or workspace directory
        #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        /// Show direct and subtree accounting for one task id
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
        /// Group run totals in text/JSON output
        #[arg(long, value_enum, default_value = "node")]
        by: CostGroup,
    },
    /// Render a self-contained HTML flow visualization of a plan or workspace
    Viz {
        /// Path to the markdown plan file (.rhei.md) or a workspace directory
        #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        /// Write the HTML here (default: <input>.html, or rhei-viz.html for a workspace)
        #[arg(long, short, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Open the rendered file in the default browser
        #[arg(long)]
        open: bool,
    },
    /// Inspect, prune, or continue from session snapshots
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// List available templates
    Templates {
        /// Emit the template list as JSON instead of plain text
        #[arg(long)]
        json: bool,
        /// Filter by discovery source: project, user, or all
        #[arg(
            long,
            default_value = "all",
            value_name = "SOURCE",
            add = ArgValueCompleter::new(complete_template_source)
        )]
        source: String,
    },
    /// Instantiate a template into a concrete plan or workspace
    Instantiate {
        /// Template name or path to a template directory
        #[arg(
            value_name = "TEMPLATE",
            add = ArgValueCompleter::new(templates::complete_template_reference)
        )]
        template: String,
        /// Set an input value (repeatable)
        #[arg(
            long = "set",
            value_name = "KEY=VALUE",
            add = ArgValueCompleter::new(templates::complete_template_set_value)
        )]
        set_values: Vec<String>,
        /// Set an input value from file contents (repeatable)
        #[arg(
            long = "set-file",
            value_name = "KEY=PATH",
            add = ArgValueCompleter::new(templates::complete_template_set_file)
        )]
        set_files: Vec<String>,
        /// Load input values from a YAML or JSON file (repeatable)
        #[arg(long, value_name = "FILE", add = ArgValueCompleter::new(complete_values_path))]
        values: Vec<PathBuf>,
        /// Output directory
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_any_path))]
        output: Option<PathBuf>,
        /// Instantiate and immediately begin execution
        ///
        /// Pass `rhei run` options after `--`, for example:
        /// `rhei instantiate my-template --execute -- --parallel 4`.
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
        /// Positional input values or KEY=VALUE assignments
        #[arg(
            value_name = "INPUT",
            num_args = 0..,
            add = ArgValueCompleter::new(templates::complete_template_input_arg)
        )]
        input_args: Vec<String>,
    },
    /// Transition the next ready task to the next state
    ///
    /// Finds the first task whose prerequisites are satisfied, transitions it
    /// forward one step, and prints the task details with state-machine
    /// instructions so an agent knows exactly what to do.
    Next {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        /// Target a specific task instead of auto-selecting
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
        /// Read-only: print the next claimable task without claiming or
        /// advancing state. Does not write `**Assignee:**` or acquire a lock.
        #[arg(long)]
        peek: bool,
    },
    /// Complete a task: transition to terminal state, write result file,
    /// link it from the task, and remove the assignee.
    Complete {
        /// Path to the markdown plan file (.rhei.md)
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: PathBuf,
        /// Task identifier (number or name)
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: String,
        /// Result message written to `runtime/results/<task-id>.md`
        #[arg(long)]
        result: String,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Reset a plan or workspace to the initial state
    Reset {
        /// Path to the markdown plan file (.rhei.md) or workspace directory
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
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
            default_value = "rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer",
            add = ArgValueCompleter::new(complete_skill_name)
        )]
        skills: Vec<String>,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: CompletionShell,
        /// Write completions to the shell's default completion location
        #[arg(long)]
        install: bool,
        /// Install into the current user's shell configuration directories
        #[arg(long, conflicts_with = "system")]
        user: bool,
        /// Install into system-wide completion directories
        #[arg(long, conflicts_with = "user")]
        system: bool,
        /// Write completions to an explicit path
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_any_path))]
        output: Option<PathBuf>,
        /// Print the destination path without writing files
        #[arg(long)]
        dry_run: bool,
    },
}
