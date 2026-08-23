
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
}

#[test]
fn run_poll_self_loop_schedules_next_attempt_and_clears_on_exit() {
    let dir = unique_temp_dir("run-poll");
    let script = write_python_agent(
        &dir,
        "poll.py",
        r#"# §FS-rhei-states.3.3: exit 0 finishes the ticket, so record why first.
result('## Result\n\nExternal status is ready.\n')
marker = pathlib.Path('runtime') / 'polled-once'
if marker.is_file():
    sys.exit(0)
write(marker, '')
sys.exit(75)
"#,
    );
    let machine = format!(
        r#"name: run-poll-test
version: 1
states:
  waiting:
    description: Poll until ready
    program:
      command: {command}
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
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Poll Run

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_poll_max_attempts_counts_the_completed_attempt_before_self_looping() {
    let dir = unique_temp_dir("run-poll-max-attempts");
    let script = write_python_agent(
        &dir,
        "poll.py",
        r#"append(pathlib.Path('runtime') / 'attempts.txt', 'attempt')
sys.exit(75)
"#,
    );
    let machine = format!(
        r#"name: run-poll-max-attempts-test
version: 1
states:
  waiting:
    description: Poll once
    program:
      command: {command}
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
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Poll Once

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_poll_allows_self_loop_until_max_attempt_cap() {
    let dir = unique_temp_dir("run-poll-max-attempts-cap");
    let script = write_python_agent(
        &dir,
        "poll.py",
        r#"append(pathlib.Path('runtime') / 'attempts.txt', 'attempt\n')
sys.exit(75)
"#,
    );
    let machine = format!(
        r#"name: run-poll-max-attempts-cap-test
version: 1
states:
  waiting:
    description: Poll until attempts are exhausted
    program:
      command: {command}
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
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Poll Three Times

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_poll_program_uses_condition_only_transitions_after_success() {
    let dir = unique_temp_dir("run-program-poll-condition");
    let script = write_python_agent(
        &dir,
        "poll.py",
        r#"append(pathlib.Path('runtime') / 'attempts.txt', 'attempt\n')
result('## Result\n\nPolled without a verdict.\n')
"#,
    );
    let machine = format!(
        r#"name: run-program-poll-condition-test
version: 1
states:
  waiting:
    description: Poll with successful condition-only transitions
    # §FS-rhei-states.3.3: the exhaustion edge is terminal, so the program —
    # the worker here — records the outcome on every attempt.
    program:
      command: {command}
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
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Poll Success Conditions

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_program_fast_nonzero_with_timeout_uses_exit_code_transition() {
    let dir = unique_temp_dir("run-program-nonzero-timeout");
    let script = write_python_agent(&dir, "build.py", "sys.exit(2)\n");
    let machine = format!(
        r#"name: run-program-nonzero-timeout-test
version: 1
states:
  build:
    description: Build artifact
    program:
      command: {command}
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
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Fast Failure

## Tasks

### Task 1: Build artifact
**State:** build
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_program_timeout_transition_ignores_missing_success_outputs() {
    let dir = unique_temp_dir("run-program-timeout-output");
    let script = write_python_agent(&dir, "build.py", "time.sleep(5)\n");
    let machine = format!(
        r#"name: run-program-timeout-output-test
version: 1
states:
  build:
    description: Build artifact
    program:
      command: {command}
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
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Timeout Failure

## Tasks

### Task 1: Build artifact
**State:** build
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_defers_program_tasks_in_default_non_concurrent_state() {
    let dir = unique_temp_dir("run-program-concurrency");
    let script = write_python_agent(
        &dir,
        "build.py",
        r#"append(pathlib.Path('runtime') / 'order.txt', env('RHEI_TASK_ID') + '\n')
result('## Result\n\nBuilt.\n')
"#,
    );
    let machine = format!(
        r#"name: run-program-concurrency-test
version: 1
states:
  build:
    description: Build artifact
    # §FS-rhei-states.3.3: exit 0 finishes the ticket, so record why.
    program:
      command: {command}
  completed:
    description: Done
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Program Concurrency

## Tasks

### Task 1: Build one
**State:** build

### Task 2: Build two
**State:** build

### Task 3: Build three
**State:** build
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

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
}

#[test]
fn run_executes_relative_callback_from_state_machine_directory() {
    let dir = unique_temp_dir("run-relative-callback");
    let workspace_dir = dir.join("examples");
    let machine_dir = workspace_dir.join("script-agent-team");
    fs::create_dir_all(&machine_dir).expect("create machine dir");

    let plan = r#"# Rhei: Relative Callback

## Tasks

### Task 1: Bootstrap
**State:** pending
"#;
    // The script path stays relative — resolving it against the state machine's
    // directory is what this test is about — so only the interpreter's name is
    // filled in.
    let machine = format!(
        r#"name: relative-callback
version: 1
states:
  pending:
    initial: true
  completed:
    final: true
transitions:
  - from: pending
    to: completed
    on_leave: "cli:{python} ./workflow.py"
"#,
        python = python_command()
    );

    let plan_path = write_fixture_file(&workspace_dir, "release-automation.rhei.md", plan);
    write_fixture_file(&machine_dir, "team-states.yaml", &machine);
    write_python_agent(
        &machine_dir,
        "workflow.py",
        r#"plan_path = pathlib.Path(env('RHEI_PLAN_PATH'))
write(plan_path.parent / 'runtime' / 'plan-path.txt', str(plan_path) + '\n')
"#,
    );

    let result = run_run_command_in_dir(
        &workspace_dir,
        Path::new("release-automation.rhei.md"),
        Path::new("script-agent-team/team-states.yaml"),
        &[],
    );

    assert!(
        result.status.success(),
        "run should succeed with callbacks relative to the state machine path\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Task release-automation.1 transitioned: 'pending' → 'completed'"),
        "expected transition output; got:\n{}",
        result.stdout
    );

    let recorded_plan_path = fs::read_to_string(workspace_dir.join("runtime/plan-path.txt"))
        .expect("read callback output");
    assert_eq!(
        Path::new(recorded_plan_path.trim()),
        rhei_core::platform::canonical_path(&plan_path).expect("canonicalize plan path"),
        "callbacks should receive an absolute plan path",
    );
}

#[test]
fn run_executes_all_models_callbacks_without_agent_configuration() {
    let dir = unique_temp_dir("run-all-models-callback");
    let plan = r#"# Rhei: Multi-Model Callback

## Tasks

### Task review-seed: Review specs
**State:** review
"#;
    let machine = format!(
        r#"name: multi-model-callback
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
        path: runtime/{{model}}-findings.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
    on_leave: "cli:{python} ./workflow.py"
"#,
        python = python_command()
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    write_python_agent(
        &dir,
        "workflow.py",
        r#"model = env('RHEI_MODEL')
if not model:
    print('RHEI_MODEL must be set', file=sys.stderr)
    sys.exit(1)
runtime_dir = pathlib.Path(env('RHEI_PLAN_PATH')).parent / 'runtime'
append(runtime_dir / 'models.txt', model + '\n')
write(runtime_dir / (model + '-findings.md'), '# Findings for {}\n'.format(model))
"#,
    );

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
}
