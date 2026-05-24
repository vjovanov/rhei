mod completions_tests;
mod examples_tests;
mod install_skills_tests;
mod next_tests;
mod run_tests;
mod snapshot_tests;
mod template_example_sync_tests;
mod templates_tests;
mod transition_tests;
mod validate_retry_cache_tests;

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

pub fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rhei-integ-{prefix}-{nanos}"));
    fs::create_dir_all(&dir).expect("temporary directory should be created");
    dir
}

pub fn unique_scratchpad_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = repo_root().join("scratchpad").join(format!("rhei-integ-{prefix}-{nanos}"));
    fs::create_dir_all(&dir).expect("scratchpad directory should be created");
    dir
}

pub fn write_fixture_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("fixture file should be written");
    path
}

/// Set up a single-file test: returns (temp_dir, plan_path, machine_path).
pub fn setup_single_file(prefix: &str, plan: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, plan_path, machine_path)
}

/// Set up a directory workspace. Returns (workspace_root, machine_path).
pub fn create_workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
) -> (PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        fs::write(tasks_dir.join(name), content).expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (ws, machine_path)
}

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("e2e").join("fixtures").join(name)
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

pub fn copy_workspace_fixture(prefix: &str, fixture_name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = unique_scratchpad_dir(prefix);
    let workspace_path = dir.join(fixture_name);
    copy_dir_recursive(&fixture_path(fixture_name), &workspace_path);
    let machine_path = workspace_path.join("team-states.yaml");
    (dir, workspace_path, machine_path)
}

/// Run an arbitrary rhei subcommand.
pub fn run_cli(
    subcommand: &str,
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", isolated_home_for(plan_path));
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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", isolated_home_for(plan_path));
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
            // JSON id now has shape { "path": "...", "segments": [...] }.
            t["id"]["path"].as_str() == Some(task_id)
        })
        .unwrap_or_else(|| panic!("Task {} not found in rendered JSON", task_id));
    let state = task["state"].as_str().expect("state field");
    assert_eq!(state, expected, "Task {} should be '{}', got '{}'", task_id, expected, state);
}

pub fn assert_success(result: &CliRun) {
    assert!(
        result.status.success(),
        "command should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
