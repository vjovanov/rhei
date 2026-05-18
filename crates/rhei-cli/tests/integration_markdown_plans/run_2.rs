
#[test]
fn run_ready_set_requires_state_inputs() {
    let machine = r#"name: run-inputs-test
version: 1
states:
  review:
    description: Review only after input exists
    initial: true
    inputs:
      - name: brief
        path: runtime/brief.md
  completed:
    description: Done
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Input Gate

## Tasks

### Task 1: Needs brief
**State:** review
"#;

    let dir = unique_temp_dir("run-inputs");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let blocked = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        !blocked.status.success(),
        "run should halt when required inputs keep the task out of the ready set\nstdout:\n{}\nstderr:\n{}",
        blocked.stdout,
        blocked.stderr
    );
    let unchanged = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(unchanged, plan);

    fs::create_dir_all(dir.join("runtime")).expect("runtime dir");
    fs::write(dir.join("runtime/brief.md"), "ready").expect("input");
    let unblocked = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        unblocked.status.success(),
        "run should proceed once the input exists\nstdout:\n{}\nstderr:\n{}",
        unblocked.stdout,
        unblocked.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_poll_self_loop_schedules_next_attempt_and_clears_on_exit() {
    let machine = r#"name: run-poll-test
version: 1
states:
  waiting:
    description: Poll until ready
    program: "bash ./poll.sh"
    poll:
      interval: 1s
      max_attempts: 3
  completed:
    description: Done
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: completed
    exit_code: 0
"#;
    let plan = r#"# Rhei: Poll Run

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
if [ -f runtime/polled-once ]; then
  exit 0
fi
mkdir -p runtime
touch runtime/polled-once
exit 75
"#;

    let dir = unique_temp_dir("run-poll");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "poll.sh", script);
    let mut perms = fs::metadata(&script_path).expect("stat poll").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod poll");
    }

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "poll run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "completed");
    let metadata = format!("{:?}", rhei.metadata);
    assert!(
        !metadata.contains("pollNextAttemptAt") && !metadata.contains("stateVisits"),
        "poll metadata should be cleared after non-self-loop exit; got {metadata}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_executes_relative_callback_from_state_machine_directory() {
    let dir = unique_temp_dir("run-relative-callback");
    let workspace_dir = dir.join("examples");
    let machine_dir = workspace_dir.join("bash-agent-team");
    fs::create_dir_all(&machine_dir).expect("create machine dir");

    let plan = r#"# Rhei: Relative Callback

## Tasks

### Task 1: Bootstrap
**State:** pending
"#;
    let machine = r#"name: relative-callback
version: 1
states:
  pending:
    initial: true
  completed:
    final: true
transitions:
  - from: pending
    to: completed
    on_leave: "cli:bash ./workflow.sh"
"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$(dirname "$RHEI_PLAN_PATH")/runtime"
printf '%s\n' "$RHEI_PLAN_PATH" > "$(dirname "$RHEI_PLAN_PATH")/runtime/plan-path.txt"
"#;

    let plan_path = write_fixture_file(&workspace_dir, "release-automation.rhei.md", plan);
    write_fixture_file(&machine_dir, "team-states.yaml", machine);
    let script_path = write_fixture_file(&machine_dir, "workflow.sh", script);
    let mut perms = fs::metadata(&script_path).expect("stat workflow").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod workflow");
    }

    let result = run_run_command_in_dir(
        &workspace_dir,
        Path::new("release-automation.rhei.md"),
        Path::new("bash-agent-team/team-states.yaml"),
        &[],
    );

    assert!(
        result.status.success(),
        "run should succeed with callbacks relative to the state machine path\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Task 1 transitioned: 'pending' → 'completed'"),
        "expected transition output; got:\n{}",
        result.stdout
    );

    let recorded_plan_path = fs::read_to_string(workspace_dir.join("runtime/plan-path.txt"))
        .expect("read callback output");
    assert_eq!(
        Path::new(recorded_plan_path.trim()),
        plan_path.canonicalize().expect("canonicalize plan path"),
        "callbacks should receive an absolute plan path",
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_executes_all_models_callbacks_without_agent_configuration() {
    let dir = unique_temp_dir("run-all-models-callback");
    let plan = r#"# Rhei: Multi-Model Callback

## Tasks

### Task review-seed: Review specs
**State:** review
"#;
    let machine = r#"name: multi-model-callback
version: 1
models:
  - claude
  - codex
states:
  review:
    initial: true
    all_models: [claude, codex]
    outputs:
      - name: findings
        path: runtime/{model}-findings.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
    on_leave: "cli:bash ./workflow.sh"
"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
: "${RHEI_MODEL:?RHEI_MODEL must be set}"
runtime_dir="$(dirname "$RHEI_PLAN_PATH")/runtime"
mkdir -p "$runtime_dir"
printf '%s\n' "$RHEI_MODEL" >> "$runtime_dir/models.txt"
printf '# Findings for %s\n' "$RHEI_MODEL" > "$runtime_dir/$RHEI_MODEL-findings.md"
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "workflow.sh", script);
    let mut perms = fs::metadata(&script_path).expect("stat workflow").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod workflow");
    }

    let result = run_run_command(&plan_path, &machine_path, &["--no-agent"]);

    assert!(
        result.status.success(),
        "run should succeed for callback-only all_models state\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei
        .tasks
        .iter()
        .find(|task| task.id == TaskId::named("review-seed"))
        .expect("review-seed exists");
    assert_eq!(task.state.as_str(), "completed");

    let models = fs::read_to_string(dir.join("runtime/models.txt")).expect("read model log");
    assert_eq!(models, "claude\ncodex\n");
    assert!(dir.join("runtime/claude-findings.md").exists(), "claude artifact should exist");
    assert!(dir.join("runtime/codex-findings.md").exists(), "codex artifact should exist");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_skips_already_completed_tasks() {
    let plan = r#"# Rhei: Already Done

## Tasks

### Task 1: Done
**State:** completed

### Task 2: Also done
**State:** completed
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("run-already-done");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // No transitions should be made.
    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "should report nothing to advance; got:\n{}",
        result.stdout
    );

    // File should be unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan, "file should not be modified");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_no_callbacks_flag_skips_callbacks() {
    let machine = r#"name: run-nocb-test
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "cli:exit 1"
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: No Callbacks Run

## Tasks

### Task 1: Should advance
**State:** pending
"#;

    let dir = unique_temp_dir("run-no-callbacks");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run --no-callbacks should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Task should reach completed despite the failing callback.
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1");
    assert_eq!(task.state.as_str(), "completed", "task should be completed with --no-callbacks");

    fs::remove_dir_all(dir).expect("cleanup");
}

