const TRANSITION_STATE_MACHINE: &str = r#"name: transition-test
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task being worked on
  completed:
    description: Task finished
    final: true
  cancelled:
    description: Task abandoned
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
  - from: "*"
    to: cancelled
"#;

const TRANSITION_PLAN: &str = r#"# Rhei: Transition Test

## Tasks

### Task 1: First task
**State:** pending

### Task 2: Second task
**State:** in-progress
**Prior:** Task 1
"#;

const COUNTED_LOOP_STATE_MACHINE: &str = r#"name: counted-loop
version: 1
states:
  pending:
    description: ready
    initial: true
  agent-review:
    description: review
    visits: 2
  agent-review-fix:
    description: fix
  human-review:
    description: human gate
  completed:
    description: done
    final: true
transitions:
  - from: pending
    to: agent-review
  - from: agent-review
    to: agent-review-fix
    condition: visitCount < visits
  - from: agent-review
    to: human-review
    condition: visitCount >= visits
  - from: agent-review-fix
    to: agent-review
  - from: human-review
    to: completed
"#;

const COUNTED_LOOP_PLAN: &str = r#"# Rhei: Counted Review Loop

## Tasks

### Task 1: Review me
**State:** pending
"#;

const COMPLETE_STATE_MACHINE: &str = r#"name: complete-test-machine
version: 1
states:
  pending:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
  cancelled:
    description: Task cancelled
    final: true
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
"#;

fn run_transition(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(machine_path)
        .arg("transition")
        .arg(plan_path)
        .arg("--task")
        .arg(task)
        .arg("--from")
        .arg(from)
        .arg("--to")
        .arg(to)
        .output()
        .expect("transition command should run");

    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_complete(plan_path: &Path, machine_path: &Path, task: &str, result_msg: &str) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(machine_path)
        .arg("complete")
        .arg(plan_path)
        .arg("--task")
        .arg(task)
        .arg("--result")
        .arg(result_msg)
        .arg("--no-callbacks")
        .output()
        .expect("complete command should run");

    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn transition_succeeds_and_updates_file() {
    let dir = unique_temp_dir("transition-success");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "in-progress");

    assert!(
        result.status.success(),
        "transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("pending"),
        "stdout should mention old state; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("in-progress"),
        "stdout should mention new state; got:\n{}",
        result.stdout
    );

    // Verify the file was actually updated.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task1.state.as_str(), "in-progress");

    // Task 2 should be untouched.
    let task2 = rhei.tasks.iter().find(|t| t.id == TaskId::number(2)).expect("Task 2 exists");
    assert_eq!(task2.state.as_str(), "in-progress");

    let result_log =
        fs::read_to_string(dir.join("runtime/results/1.md")).expect("read transition result log");
    assert!(result_log.contains("## pending \u{2192} in-progress"));

    let completed = run_complete(&plan_path, &machine_path, "1", "done");
    assert!(
        completed.status.success(),
        "complete should succeed after explicit transition\nstdout:\n{}\nstderr:\n{}",
        completed.stdout,
        completed.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read completed plan");
    assert!(
        updated.contains("> **Result:** [1](runtime/results/1.md)"),
        "completion should link result even when transition created the audit file first:\n{updated}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_counted_loop_updates_metadata_and_blocks_exhausted_reentry() {
    let dir = unique_temp_dir("transition-counted-loop");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", COUNTED_LOOP_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let first = run_transition(&plan_path, &machine_path, "1", "pending", "agent-review");
    assert!(first.status.success(), "initial review transition should succeed: {}", first.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after first transition");
    let rhei = parse(&updated).expect("parse updated plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(1)
    );

    let fail_then_fix =
        run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(
        fail_then_fix.status.success(),
        "review -> fix should succeed: {}",
        fail_then_fix.stderr
    );

    let reenter =
        run_transition(&plan_path, &machine_path, "1", "agent-review-fix", "agent-review");
    assert!(reenter.status.success(), "fix -> review should succeed: {}", reenter.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after re-entry");
    let rhei = parse(&updated).expect("parse re-entered plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(2)
    );

    let exhausted =
        run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(!exhausted.status.success(), "exhausted review loop should reject re-entry");
    assert!(
        normalize_for_assertions(&exhausted.stderr).contains("evaluated to false"),
        "expected loop-budget rejection, got:\n{}",
        exhausted.stderr
    );

    let escalate = run_transition(&plan_path, &machine_path, "1", "agent-review", "human-review");
    assert!(
        escalate.status.success(),
        "human review escalation should succeed: {}",
        escalate.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_from_authored_counted_state_treats_start_as_first_visit() {
    let dir = unique_temp_dir("transition-authored-counted-loop");
    let plan = r#"# Rhei: Authored Counted Review Loop

## Tasks

### Task 1: Start in review
**State:** agent-review
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let to_fix = run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(to_fix.status.success(), "review -> fix should succeed: {}", to_fix.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after leaving authored review");
    let rhei = parse(&updated).expect("parse updated plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(1)
    );

    let reenter =
        run_transition(&plan_path, &machine_path, "1", "agent-review-fix", "agent-review");
    assert!(reenter.status.success(), "fix -> review should succeed: {}", reenter.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after re-entering review");
    assert!(
        updated.contains("**State:** agent-review-2"),
        "expected visible counted visit suffix after re-entry:\n{}",
        updated
    );
    let rhei = parse(&updated).expect("parse re-entered plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(2)
    );

    let exhausted =
        run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(!exhausted.status.success(), "second re-entry should exhaust the visit budget");
    assert!(
        normalize_for_assertions(&exhausted.stderr).contains("evaluated to false"),
        "expected loop-budget rejection, got:\n{}",
        exhausted.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_accepts_counted_state_suffix_within_budget() {
    let input = r#"# Rhei: Counted State Suffix
## Tasks

### Task 1: Review
**State:** agent-review-2
"#;

    let rhei = parse(input).expect("parse ok");
    let report = validate_with_machine(
        &rhei,
        &rhei_validator::StateMachine::from_yaml_str(COUNTED_LOOP_STATE_MACHINE)
            .expect("state machine"),
    );

    assert!(
        !report.has_errors(),
        "counted state suffix within budget should validate: {:?}",
        report.errors
    );
}

#[test]
fn transition_fails_on_cas_conflict() {
    let dir = unique_temp_dir("transition-cas");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    // Task 1 is in "pending", but we claim it's "in-progress".
    let result = run_transition(&plan_path, &machine_path, "1", "in-progress", "completed");

    assert!(!result.status.success(), "transition should fail on CAS conflict");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(normalized.contains("conflict"), "should report conflict; got:\n{}", result.stderr);
    assert!(
        normalized.contains("pending"),
        "should mention actual state 'pending'; got:\n{}",
        result.stderr
    );

    // File should be unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, TRANSITION_PLAN);

    fs::remove_dir_all(dir).expect("cleanup");
}
