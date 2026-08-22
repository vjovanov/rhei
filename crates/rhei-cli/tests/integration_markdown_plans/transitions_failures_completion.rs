
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
    // `cancelled` is `final: true`, so the move carries its reason.
    // §FS-rhei-states.3.3
    let result = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "pending",
        "cancelled",
        "Abandoned by hand.",
    );

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

/// `rhei complete` still refuses a parent with an open child — but the refusal
/// now comes from the shared transition path rather than a private check, so
/// the wording is the verb-neutral one every path produces.
// §FS-rhei-transition-cmd.3.1 §FS-rhei-complete.4
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
        normalized.contains("cannot enter terminal state 'completed' while descendant tasks"),
        "expected the shared descendants-first guard in stderr, got:\n{}",
        result.stderr
    );
    // One format across `rhei next --task`, `rhei transition`, and
    // `rhei complete`. §FS-rhei-transition-cmd.3.1
    assert!(
        normalized.contains("Task plan.1.1 (pending)"),
        "expected the offending child rendered as `Task <id> (<state>)`, got:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "pending");
    assert_eq!(task.children[0].state.as_str(), "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The plan file is the artifact humans read and review in a diff, so
/// completion must not degrade its spacing. The result block used to land with
/// two blank lines above it and none below, butting the next heading against
/// the blockquote and getting worse with every completion.
#[test]
fn complete_surrounds_the_result_block_with_exactly_one_blank_line() {
    let plan = r#"# Rhei: Result Spacing

## Tasks

### Task 1: First
**State:** pending

Body of one.

### Task 2: Last
**State:** pending

Body of two.
"#;

    let dir = unique_temp_dir("complete-result-spacing");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    for task in ["1", "2"] {
        let result = run_complete(&plan_path, &machine_path, task, "done");
        assert!(result.status.success(), "complete {task} failed:\n{}", result.stderr);
    }

    let updated = fs::read_to_string(&plan_path).expect("read completed plan");
    assert!(
        updated.contains("Body of one.\n\n> **Result:** [plan.1](runtime/results/plan.1.md)\n\n### Task 2: Last"),
        "a mid-file result block needs exactly one blank line on each side; got:\n{updated}"
    );
    assert!(
        updated.ends_with("> **Result:** [plan.2](runtime/results/plan.2.md)\n"),
        "an end-of-file result block must not add trailing blank lines; got:\n{updated}"
    );
    assert!(
        !updated.contains("\n\n\n"),
        "completion must not introduce doubled blank lines; got:\n{updated}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-complete.4: completing ahead of a prerequisite makes the ticket
/// terminal, so the violation would drop out of readiness and never resurface.
#[test]
fn complete_rejects_task_with_unsatisfied_prior() {
    let plan = r#"# Rhei: Prior Completion Guard

## Tasks

### Task 1: First
**State:** pending

### Task 2: Second
**State:** pending
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("complete-unsatisfied-prior");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    let result = run_complete(&plan_path, &machine_path, "2", "done");

    assert!(!result.status.success(), "complete should fail when a prior is unsatisfied");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("cannot be completed while its prerequisites are unsatisfied"),
        "expected prior guard in stderr, got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("Task plan.1 (pending)"),
        "expected blocking prior and its state in stderr, got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("rhei transition"),
        "expected the deliberate-override escape hatch in stderr, got:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(2)).expect("Task 2 exists");
    assert_eq!(task.state.as_str(), "pending", "the rejected completion must not write state");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The guard must not fire once the prior is genuinely satisfied.
#[test]
fn complete_accepts_task_once_prior_is_terminal() {
    let plan = r#"# Rhei: Prior Completion Guard

## Tasks

### Task 1: First
**State:** completed

### Task 2: Second
**State:** pending
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("complete-satisfied-prior");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    let result = run_complete(&plan_path, &machine_path, "2", "done");

    assert!(
        result.status.success(),
        "complete should succeed when the prior is terminal\nstderr:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(2)).expect("Task 2 exists");
    assert_eq!(task.state.as_str(), "completed");

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
        updated.contains("> **Result:** [plan.1](runtime/results/plan.1.md)"),
        "expected result link in updated plan:\n{}",
        updated
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_redirected_to_non_terminal_state_does_not_write_completion_artifacts() {
    let machine_yaml = r#"name: complete-redirect
version: 1
states:
  pending:
    description: Ready
  in-progress:
    description: Still open
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
    on_leave: 'cli:printf ''{"success": true, "nextState": "in-progress"}'''
  - from: pending
    to: in-progress
"#;
    let plan = r#"# Rhei: Completion Redirect

## Tasks

### Task 1: Alpha
**State:** pending
**Assignee:** codex
"#;

    let dir = unique_temp_dir("complete-redirect-non-terminal");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let output = rhei_command()
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("complete")
        .arg(&plan_path)
        .arg("--task")
        .arg("1")
        .arg("--result")
        .arg("done")
        .output()
        .expect("complete command should run");
    let result = CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };

    assert!(!result.status.success(), "redirected complete should fail");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not a successful") && normalized.contains("completion state"),
        "stderr should explain non-completion redirect; got:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "in-progress");
    assert!(
        updated.contains("**Assignee:** codex"),
        "assignee should remain when complete does not finalize:\n{}",
        updated
    );
    assert!(
        !updated.contains("> **Result:**"),
        "result block should not be written after non-completion redirect:\n{}",
        updated
    );
    assert!(
        !dir.join("runtime/results/1.md").exists(),
        "completion result file should not be written"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

// --- Callback execution integration tests ---
