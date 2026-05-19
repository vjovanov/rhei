
#[test]
fn transition_fails_on_invalid_transition() {
    let dir = unique_temp_dir("transition-invalid");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    // pending → completed is not a declared transition.
    let result = run_transition(&plan_path, &machine_path, "1", "pending", "completed");

    assert!(!result.status.success(), "transition should fail for disallowed transition");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not allowed"),
        "should report transition not allowed; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_fails_on_nonexistent_task() {
    let dir = unique_temp_dir("transition-missing");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "99", "pending", "in-progress");

    assert!(!result.status.success(), "transition should fail for nonexistent task");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not found"),
        "should report task not found; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_works_with_named_task_id() {
    let plan = r#"# Rhei: Named Task Test

## Tasks

### Task setup: Initialize project
**State:** pending

### Task build: Build artifacts
**State:** pending
**Prior:** Task setup
"#;

    let dir = unique_temp_dir("transition-named");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "setup", "pending", "in-progress");

    assert!(
        result.status.success(),
        "transition should succeed for named task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task =
        rhei.tasks.iter().find(|t| t.id == TaskId::named("setup")).expect("Task setup exists");
    assert_eq!(task.state.as_str(), "in-progress");

    // Task build should be untouched.
    let build =
        rhei.tasks.iter().find(|t| t.id == TaskId::named("build")).expect("Task build exists");
    assert_eq!(build.state.as_str(), "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_wildcard_from_allows_any_source() {
    let dir = unique_temp_dir("transition-wildcard");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    // The wildcard `from: "*"` → cancelled should allow pending → cancelled.
    let result = run_transition(&plan_path, &machine_path, "1", "pending", "cancelled");

    assert!(
        result.status.success(),
        "wildcard transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task1.state.as_str(), "cancelled");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn states_profile_allowed_rejects_manual_transition_destination() {
    let machine_yaml = r#"name: profile-transition-guard
version: 3
states:
  pending:
    description: Not started
  review:
    description: Globally valid but not allowed for simple tasks
  completed:
    description: Done
    final: true
profiles:
  simple:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: simple
  default: simple
transitions:
  - from: pending
    to: review
  - from: pending
    to: completed
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Profile Transition Guard

## Tasks

### Task 1: Simple task
**State:** pending
"#;
    let dir = unique_temp_dir("states-profile-manual-transition");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "review");

    assert!(
        !result.status.success(),
        "profile-disallowed transition target should fail"
    );
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not allowed") && normalized.contains("resolved") && normalized.contains("profile"),
        "stderr should explain profile allowed-state guard; got:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read unchanged plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn states_profile_allowed_skips_disallowed_automatic_transition_destination() {
    let machine_yaml = r#"name: profile-auto-transition-guard
version: 3
states:
  pending:
    description: Not started
  review:
    description: Globally valid but not allowed for simple tasks
  completed:
    description: Done
    final: true
profiles:
  simple:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: simple
  default: simple
transitions:
  - from: pending
    to: review
  - from: pending
    to: completed
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Profile Auto Transition Guard

## Tasks

### Task 1: Simple task
**State:** pending
"#;
    let dir = unique_temp_dir("states-profile-auto-transition");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should skip the disallowed transition and use the allowed target\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "completed");
    assert!(
        !result.stdout.contains("review"),
        "run output should not show the skipped disallowed state; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_rejects_parent_with_non_terminal_subtasks() {
    let plan = r#"# Rhei: Parent Completion Guard

## Tasks

### Task 1: Parent task
**State:** pending

#### Task 1.1: Open item
**State:** pending
"#;

    let dir = unique_temp_dir("complete-open-subtasks");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    let result = run_complete(&plan_path, &machine_path, "1", "done");

    assert!(!result.status.success(), "complete should fail when children are non-terminal");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("cannot be completed while child tasks remain non-terminal"),
        "expected child-task guard in stderr, got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("Task 1.1"),
        "expected offending child task id in stderr, got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("('Open item') [pending]"),
        "expected offending child task state in stderr, got:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "pending");
    assert_eq!(task.children[0].state.as_str(), "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_succeeds_when_all_subtasks_are_terminal() {
    let plan = r#"# Rhei: Parent Completion Success

## Tasks

### Task 1: Parent task
**State:** pending

#### Task 1.1: Closed item
**State:** completed
"#;

    let dir = unique_temp_dir("complete-terminal-subtasks");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    let result = run_complete(&plan_path, &machine_path, "1", "done");

    assert!(
        result.status.success(),
        "complete should succeed when subtasks are terminal\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "completed");
    assert_eq!(task.children[0].state.as_str(), "completed");
    assert!(
        updated.contains("> **Result:** [1](runtime/results/1.md)"),
        "expected result link in updated plan:\n{}",
        updated
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

// --- Callback execution integration tests ---
