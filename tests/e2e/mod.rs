mod accounting_contract_tests;
mod accounting_prices_tests;
mod completions_tests;
mod error_guidance_tests;
mod examples_tests;
mod handoff_tests;
mod headless_recovery_tests;
mod headless_stop_ownership_tests;
mod headless_support;
mod headless_tests;
mod headless_undecided_tests;
mod install_skills_tests;
mod laid_output_root_tests;
mod list_ready_tests;
mod memory_map_tests;
mod memory_prompt_tests;
mod new_guard_tests;
mod new_tests;
mod new_write_tests;
mod next_tests;
mod run_lock_wait_tests;
mod run_shell_program_tests;
mod run_signals_tests;
mod run_tests;
mod snapshot_tests;
mod summary_tests;
mod supervised_delivery_tests;
mod supervision_barrier_tests;
mod supervision_empty_visit_tests;
mod supervision_next_tests;
mod supervision_no_spawn_release_tests;
mod supervision_release_rule_tests;
mod supervision_surfaces_tests;
mod supervision_tests;
mod template_example_sync_tests;
mod templates_render_tests;
mod templates_tests;
mod terminal_result_attempt_tests;
mod terminal_result_fanout_tests;
mod terminal_result_redirect_tests;
mod terminal_result_stall_tests;
mod terminal_result_tests;
mod transition_tests;
mod validate_retry_cache_tests;

// Shared with the `integration_markdown_plans` harness, which cannot see this
// module tree and `include!`s the same file.
#[path = "../support/binaries.rs"]
mod binaries;
#[path = "../support/python_fixture.rs"]
mod python_fixture;
#[path = "../support/test_dir.rs"]
mod test_dir;

pub use python_fixture::{
    fixture_command, fixture_command_line, python_command, write_python_agent,
};

/// A `cli:` callback that runs one line of Python, spelled as a YAML scalar.
///
/// A callback is a command line for the platform's own shell, and the two
/// shells share almost no vocabulary — `printf` is `sh`, not `cmd`. The code
/// goes inside one pair of double quotes and quotes its own strings with
/// `'…'`, which both shells hand through unchanged, and `serde_json` spells
/// the result as a YAML double-quoted scalar.

// §FS-rhei-programs.1.1
pub fn python_callback_yaml(code: &str) -> String {
    serde_json::to_string(&format!("cli:{} -c \"{code}\"", python_command()))
        .expect("callback should serialize")
}
pub use binaries::rhei_binary;
pub use test_dir::TestDir;

/// The product's own quoting, so a test builds an expected command line the way
/// the product built it — POSIX quotes on Unix, `cmd`'s on Windows — instead of
/// dropping the assertion to a substring because the two platforms disagree.
// §FS-rhei-errors.2
pub use rhei_core::platform::shell_quote;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

pub const STATE_MACHINE: &str = r#"name: integration-test
version: 1
states:
  draft:
    initial: true
    description: Analysis phase
    instructions: |
      Analyze the task and write a description. Transition to pending once done.
  pending:
    description: Ready for work
    instructions: |
      Implement the task. Transition to completed when finished.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
"#;

// ---------------------------------------------------------------------------
// Plan templates (all tasks start in draft)
// ---------------------------------------------------------------------------

pub const LINEAR_PLAN: &str = r#"# Rhei: Linear Chain

## Tasks

### Task 1: First step
**State:** draft

### Task 2: Second step
**State:** draft
**Prior:** Task 1

### Task 3: Third step
**State:** draft
**Prior:** Task 2
"#;

pub const PARALLEL_PLAN: &str = r#"# Rhei: Parallel Branches

## Tasks

### Task 1: Root
**State:** draft

### Task 2: Branch A
**State:** draft
**Prior:** Task 1

### Task 3: Branch B
**State:** draft
**Prior:** Task 1
"#;

pub const INDEPENDENT_PLAN: &str = r#"# Rhei: Independent Tasks

## Tasks

### Task 1: Alpha
**State:** draft

### Task 2: Beta
**State:** draft

### Task 3: Gamma
**State:** draft
"#;

pub const SUBTASK_PLAN: &str = r#"# Rhei: Subtask Test

## Tasks

### Task 1: Parent task
**State:** draft
Some task content here.

#### Task 1.1: First subtask
**State:** draft
Subtask one content.

#### Task 1.2: Second subtask
**State:** draft
Subtask two content.
"#;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub struct CliRun {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn unique_temp_dir(prefix: &str) -> TestDir {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    TestDir::create(std::env::temp_dir().join(format!("rhei-integ-{prefix}-{nanos}")))
}

pub fn unique_scratchpad_dir(prefix: &str) -> TestDir {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    TestDir::create(repo_root().join("scratchpad").join(format!("rhei-integ-{prefix}-{nanos}")))
}

pub fn write_fixture_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("fixture file should be written");
    path
}

/// Set up a single-file test: returns (temp_dir, plan_path, machine_path).
pub fn setup_single_file(prefix: &str, plan: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, plan_path, machine_path)
}

/// Set up a directory workspace. Returns (temp_dir, workspace_root,
/// machine_path); the workspace lives inside the temp directory, so the first
/// element is what has to stay bound for the tree to outlive the setup call.
pub fn create_workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        fs::write(tasks_dir.join(name), content).expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, ws, machine_path)
}

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should have workspace parent")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create fixture directory");

    for entry in fs::read_dir(src).expect("read fixture directory") {
        let entry = entry.expect("fixture entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("fixture file type");

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
            continue;
        }

        fs::copy(&src_path, &dst_path).expect("copy fixture file");
        let permissions = fs::metadata(&src_path).expect("fixture metadata").permissions();
        fs::set_permissions(&dst_path, permissions).expect("fixture permissions");
    }
}

pub fn copy_workspace_fixture(prefix: &str, fixture_name: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_scratchpad_dir(prefix);
    let workspace_path = dir.join(fixture_name);
    copy_dir_recursive(&fixture_path(fixture_name), &workspace_path);
    let machine_path = workspace_path.join("team-states.yaml");
    // A fixture's `cli:` callbacks name their interpreter as
    // `RHEI_FIXTURE_PYTHON`, because which of `python3` and `python` exists is
    // a fact about the host and not about the fixture. The committed machine
    // is the template; the copy is what runs.
    let machine = fs::read_to_string(&machine_path).expect("fixture state machine");
    fs::write(&machine_path, machine.replace("RHEI_FIXTURE_PYTHON", python_command()))
        .expect("fixture state machine with the host's interpreter");
    (dir, workspace_path, machine_path)
}

/// Every `rhei` an end-to-end test spawns, with **both** state locations pinned
/// under `home`.
///
/// The run registry is machine-wide and every non-dry run publishes into it, so
/// a `HOME` alone leaves a test writing entries into the developer's own
/// `$XDG_STATE_HOME/rhei/runs` wherever their environment sets that variable.

// §FS-rhei-run-headless.2
pub fn rhei_command(home: impl AsRef<Path>) -> Command {
    let home = home.as_ref();
    let _ = fs::create_dir_all(home.join("state"));
    let mut cmd = Command::new(rhei_binary());
    cmd.env("HOME", home);
    cmd.env("XDG_STATE_HOME", home.join("state"));
    cmd
}

/// Run an arbitrary rhei subcommand.
pub fn run_cli(
    subcommand: &str,
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = rhei_command(isolated_home_for(plan_path));
    cmd.arg("--state-machine").arg(machine_path).arg(subcommand).arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Run an arbitrary rhei subcommand without passing `--state-machine`.
pub fn run_cli_without_machine(subcommand: &str, plan_path: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(isolated_home_for(plan_path));
    cmd.arg(subcommand).arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn isolated_home_for(plan_path: &Path) -> PathBuf {
    plan_path.parent().unwrap_or_else(|| Path::new(".")).join(".home")
}

/// Run `rhei transition`.
pub fn run_transition(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
) -> CliRun {
    run_cli(
        "transition",
        plan_path,
        machine_path,
        &["--task", task, "--from", from, "--to", to, "--no-callbacks"],
    )
}

/// Run `rhei transition --result`, which every move into a `final: true` state
/// needs unless the ticket already has a result on disk. §FS-rhei-states.3.3
pub fn run_transition_with_result(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
    result: &str,
) -> CliRun {
    run_cli(
        "transition",
        plan_path,
        machine_path,
        &["--task", task, "--from", from, "--to", to, "--result", result, "--no-callbacks"],
    )
}

/// Render the plan as JSON via `rhei render --format json --pretty` and return
/// the parsed JSON. All state assertions go through the CLI this way.
pub fn render_json(plan_path: &Path, machine_path: &Path) -> serde_json::Value {
    let result = run_cli("render", plan_path, machine_path, &["--format", "json", "--pretty"]);
    assert_success(&result);
    serde_json::from_str(&result.stdout).expect("render JSON should parse")
}

/// Assert that every task in the plan has the given state, verified via CLI.
pub fn assert_all_tasks_in_state(plan_path: &Path, machine_path: &Path, expected: &str) {
    let json = render_json(plan_path, machine_path);
    let tasks = json["tasks"].as_array().expect("tasks array");
    assert!(!tasks.is_empty(), "plan should have tasks");
    for task in tasks {
        let id = &task["id"];
        let state = task["state"].as_str().expect("state field");
        assert_eq!(state, expected, "Task {} should be '{}', got '{}'", id, expected, state);
    }
}

/// Assert a single task has the expected state, verified via CLI.
/// `task_id` can be a number (e.g. "1") or a name (e.g. "setup").
pub fn assert_task_state(plan_path: &Path, machine_path: &Path, task_id: &str, expected: &str) {
    let json = render_json(plan_path, machine_path);
    let tasks = json["tasks"].as_array().expect("tasks array");
    let task = tasks
        .iter()
        .find(|t| {
            // JSON id now has shape { "path": "...", "segments": [...] }, with the
            // path qualified by the implicit Panta rhei id (e.g. "plan.1"). Match
            // either the exact path or the local id after the rhei qualifier.
            t["id"]["path"].as_str().is_some_and(|path| {
                path == task_id || path.split_once('.').is_some_and(|(_, local)| local == task_id)
            })
        })
        .unwrap_or_else(|| panic!("Task {} not found in rendered JSON", task_id));
    let state = task["state"].as_str().expect("state field");
    assert_eq!(state, expected, "Task {} should be '{}', got '{}'", task_id, expected, state);
}

/// Assert that stderr contains `expected`, ignoring miette line wrapping and
/// decoration. Both sides are collapsed to their ASCII-graphic characters so
/// wraps inside hyphenated words (e.g. `human-review`) cannot break matches.
pub fn assert_stderr_contains(result: &CliRun, expected: &str) {
    fn collapse(text: &str) -> String {
        text.chars().filter(|c| c.is_ascii_graphic()).collect()
    }
    assert!(
        collapse(&result.stderr).contains(&collapse(expected)),
        "expected stderr to contain {:?}; got:\n{}",
        expected,
        result.stderr
    );
}

pub fn assert_success(result: &CliRun) {
    assert!(
        result.status.success(),
        "command should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
