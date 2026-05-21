
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).expect("stat poll").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod poll");
    }
    #[cfg(not(unix))]
    let _ = &script_path;

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
fn run_poll_max_attempts_counts_the_completed_attempt_before_self_looping() {
    let machine = r#"name: run-poll-max-attempts-test
version: 1
states:
  waiting:
    description: Poll once
    program: "mkdir -p runtime && printf attempt >> runtime/attempts.txt && exit 75"
    poll:
      interval: 1s
      max_attempts: 1
  exhausted:
    description: Polling exhausted
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: exhausted
    exit_code: 75
"#;
    let plan = r#"# Rhei: Poll Once

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let dir = unique_temp_dir("run-poll-max-attempts");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "poll run should route to exhaustion after one attempt\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "exhausted");
    let attempts = fs::read_to_string(dir.join("runtime/attempts.txt")).expect("read attempts");
    assert_eq!(attempts.matches("attempt").count(), 1);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_poll_allows_self_loop_until_max_attempt_cap() {
    let machine = r#"name: run-poll-max-attempts-cap-test
version: 1
states:
  waiting:
    description: Poll until attempts are exhausted
    program: "mkdir -p runtime && printf 'attempt\n' >> runtime/attempts.txt && exit 75"
    poll:
      interval: 0s
      max_attempts: 3
  exhausted:
    description: Polling exhausted
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: exhausted
    exit_code: 75
"#;
    let plan = r#"# Rhei: Poll Three Times

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let dir = unique_temp_dir("run-poll-max-attempts-cap");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "poll run should route to exhaustion after three attempts\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "exhausted");
    let attempts = fs::read_to_string(dir.join("runtime/attempts.txt")).expect("read attempts");
    assert_eq!(attempts.lines().count(), 3);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_poll_program_uses_condition_only_transitions_after_success() {
    let machine = r#"name: run-program-poll-condition-test
version: 1
states:
  waiting:
    description: Poll with successful condition-only transitions
    program: "mkdir -p runtime && printf 'attempt\n' >> runtime/attempts.txt"
    poll:
      interval: 0s
      max_attempts: 3
  exhausted:
    description: Polling exhausted
    final: true
transitions:
  - from: waiting
    to: waiting
    condition: pollAttempts < pollMaxAttempts
  - from: waiting
    to: exhausted
    condition: pollAttempts >= pollMaxAttempts
"#;
    let plan = r#"# Rhei: Poll Success Conditions

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let dir = unique_temp_dir("run-program-poll-condition");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "successful program poll should evaluate condition-only transitions\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "exhausted");
    let attempts = fs::read_to_string(dir.join("runtime/attempts.txt")).expect("read attempts");
    assert_eq!(attempts.lines().count(), 3);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_program_fast_nonzero_with_timeout_uses_exit_code_transition() {
    let machine = r#"name: run-program-nonzero-timeout-test
version: 1
states:
  build:
    description: Build artifact
    program: "exit 2"
    program_timeout: 30s
    outputs:
      - name: bundle
        path: runtime/bundle.txt
  failed-by-code:
    description: Failed by exit code
    final: true
  timed-out:
    description: Timed out
    final: true
transitions:
  - from: build
    to: failed-by-code
    exit_code: 2
  - from: build
    to: timed-out
    timeout: 30s
"#;
    let plan = r#"# Rhei: Fast Failure

## Tasks

### Task 1: Build artifact
**State:** build
"#;

    let dir = unique_temp_dir("run-program-nonzero-timeout");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "fast non-zero exit should not be treated as timeout\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "failed-by-code");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_program_timeout_transition_ignores_missing_success_outputs() {
    let machine = r#"name: run-program-timeout-output-test
version: 1
states:
  build:
    description: Build artifact
    program: "sleep 5"
    program_timeout: 1s
    outputs:
      - name: bundle
        path: runtime/bundle.txt
  timed-out:
    description: Timed out
    final: true
transitions:
  - from: build
    to: timed-out
    timeout: 1s
"#;
    let plan = r#"# Rhei: Timeout Failure

## Tasks

### Task 1: Build artifact
**State:** build
"#;

    let dir = unique_temp_dir("run-program-timeout-output");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "timeout transition should not require success outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "timed-out");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_defers_program_tasks_in_default_non_concurrent_state() {
    let machine = r#"name: run-program-concurrency-test
version: 1
states:
  build:
    description: Build artifact
    program: "mkdir -p runtime && echo $RHEI_TASK_ID >> runtime/order.txt"
  completed:
    description: Done
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
"#;
    let plan = r#"# Rhei: Program Concurrency

## Tasks

### Task 1: Build one
**State:** build

### Task 2: Build two
**State:** build

### Task 3: Build three
**State:** build
"#;

    let dir = unique_temp_dir("run-program-concurrency");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks", "--parallel", "0"]);
    assert!(
        result.status.success(),
        "program run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Deferred 2 task(s) in non-concurrent states")
            && result.stdout.contains("Deferred 1 task(s) in non-concurrent states"),
        "program tasks in the default non-concurrent state should be deferred by pass; got:\n{}",
        result.stdout
    );
    let order = fs::read_to_string(dir.join("runtime/order.txt")).expect("read order");
    assert_eq!(order.lines().count(), 3);

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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).expect("stat workflow").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod workflow");
    }
    #[cfg(not(unix))]
    let _ = &script_path;

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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).expect("stat workflow").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod workflow");
    }
    #[cfg(not(unix))]
    let _ = &script_path;

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
