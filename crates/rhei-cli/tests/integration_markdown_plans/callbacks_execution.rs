const CALLBACK_STATE_MACHINE: &str = r#"name: callback-test
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
    on_leave: "cli:echo on_leave_fired"
    on_enter: "cli:echo on_enter_fired"
  - from: in-progress
    to: completed
    on_leave: "cli:exit 1"
"#;

fn run_transition_with_flags(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.arg("--state-machine")
        .arg(machine_path)
        .arg("transition")
        .arg(plan_path)
        .arg("--task")
        .arg(task)
        .arg("--from")
        .arg(from)
        .arg("--to")
        .arg(to);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("transition command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn callback_on_leave_and_on_enter_invoked_on_transition() {
    let dir = unique_temp_dir("callback-invoked");
    let plan = r#"# Rhei: Callback Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CALLBACK_STATE_MACHINE);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        result.status.success(),
        "transition with callbacks should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Verify the file was updated.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "in-progress");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_on_leave_failure_blocks_transition() {
    let dir = unique_temp_dir("callback-blocks");
    let plan = r#"# Rhei: Callback Failure Test

## Tasks

### Task 1: Alpha
**State:** in-progress
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CALLBACK_STATE_MACHINE);

    // in-progress → completed has on_leave: "cli:exit 1" which should fail.
    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "in-progress", "completed", &[]);

    assert!(!result.status.success(), "transition should fail when on_leave callback rejects");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("on_leave") && normalized.contains("rejected"),
        "should report on_leave rejection; got:\n{}",
        result.stderr
    );

    // File should be unchanged — transition did not proceed.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn no_callbacks_flag_skips_callback_execution() {
    let dir = unique_temp_dir("callback-skip");
    let plan = r#"# Rhei: No Callbacks Test

## Tasks

### Task 1: Alpha
**State:** in-progress
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CALLBACK_STATE_MACHINE);

    // on_leave would fail (exit 1), but --no-callbacks should skip it.
    let result = run_transition_with_flags(
        &plan_path,
        &machine_path,
        "1",
        "in-progress",
        "completed",
        &["--no-callbacks"],
    );

    assert!(
        result.status.success(),
        "transition with --no-callbacks should succeed even when callback would fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_unknown_platform_produces_clear_error() {
    let machine_yaml = r#"name: bad-callback
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "js:someFunction"
"#;
    let dir = unique_temp_dir("callback-bad-platform");
    let plan = r#"# Rhei: Bad Callback Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(!result.status.success(), "transition should fail for unknown callback platform");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("unknown callback platform"),
        "should report unknown platform; got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("js:someFunction"),
        "should include the callback identifier; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_rejection_surfaces_spec_error_message() {
    // A callback returns `{"success": false, "error": "..."}` per the spec;
    // the CLI should surface the message verbatim and leave the plan unchanged.
    let machine_yaml = r#"name: spec-rejection
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
    on_leave: 'cli:printf ''{"success": false, "error": "dep not met"}'''
  - from: in-progress
    to: completed
"#;
    let dir = unique_temp_dir("callback-spec-rejection");
    let plan = r#"# Rhei: Spec Rejection Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(!result.status.success(), "spec-style rejection should fail the transition");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("dep not met"),
        "stderr should carry the callback's error message; got:\n{}",
        result.stderr
    );

    // File unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}
