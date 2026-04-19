use std::fs;

use super::*;

/// Drive a plan to completion using `next` (to claim from initial state)
/// followed by `complete` (to finish the task). This simulates the agent
/// workflow: orchestrator calls `next`, agent does work, agent calls `complete`.
fn drive_to_completion_via_next(plan_path: &std::path::Path, machine_path: &std::path::Path) {
    loop {
        let next_result = run_cli("next", plan_path, machine_path, &["--no-callbacks", "--json"]);
        if !next_result.status.success() {
            break;
        }

        let json: serde_json::Value = serde_json::from_str(&next_result.stdout).expect("next JSON");
        let task_id = json["task_id"].as_str().expect("task_id field");

        let complete_result = run_cli(
            "complete",
            plan_path,
            machine_path,
            &["--task", task_id, "--result", "done", "--no-callbacks"],
        );
        assert_success(&complete_result);
    }
}

#[test]
fn next_single_file_repeated_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("next-repeat", LINEAR_PLAN);

    drive_to_completion_via_next(&plan_path, &machine_path);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    // Verify next now fails.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "next should fail when all tasks are completed");
    assert!(
        result.stderr.contains("no tasks are ready"),
        "expected 'no tasks are ready'; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_single_file_json_output() {
    let plan = r#"# Rhei: JSON Output Test

## Tasks

### Task 1: Setup environment
**State:** draft
Configure the build system.
"#;

    let (dir, plan_path, machine_path) = setup_single_file("next-json", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("stdout should be valid JSON");

    assert_eq!(json["task_id"], "1");
    assert_eq!(json["title"], "Setup environment");
    assert_eq!(json["from_state"], "draft");
    assert_eq!(json["state"], "pending");
    assert!(json["personality"].is_null());
    assert!(json["instructions"].is_string());
    assert!(json["content"].is_string());
    assert!(json["subtasks"].is_array());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_prints_personality_in_text_and_json_when_configured() {
    let plan = r#"# Rhei: Personality Output Test

## Tasks

### Task 1: Teach concurrency
**State:** draft
Explain lock-free tradeoffs.
"#;
    let machine = r#"name: professor-demo
personality: You are an MIT professor.
version: 1
states:
  draft:
    initial: true
    description: Planning
    instructions: Analyze first.
  pending:
    description: Ready
    instructions: Teach clearly.
transitions:
  - from: draft
    to: pending
"#;

    let dir = unique_temp_dir("next-personality");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let text_result =
        run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert_success(&text_result);
    assert!(
        text_result.stdout.contains("Personality: You are an MIT professor."),
        "expected personality in text output; got:\n{}",
        text_result.stdout
    );
    assert!(
        text_result.stdout.contains("## Task 1: Teach concurrency"),
        "expected task heading in text output; got:\n{}",
        text_result.stdout
    );

    let json_result =
        run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1", "--json"]);
    assert_success(&json_result);
    let json: serde_json::Value = serde_json::from_str(&json_result.stdout).expect("parse JSON");
    assert_eq!(json["personality"], "You are an MIT professor.");
    assert_eq!(json["task_id"], "1");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_respects_dependency_order() {
    let (dir, plan_path, machine_path) = setup_single_file("next-deps", LINEAR_PLAN);

    // First next: should advance Task 1 (draft -> pending) since it has no deps.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "1", "first next should pick Task 1");
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert_task_state(&plan_path, &machine_path, "2", "draft");
    assert_task_state(&plan_path, &machine_path, "3", "draft");

    // Second next: Task 1 is already claimed and Task 2 is still blocked.
    // Auto-pick mode should not collide with already-claimed work.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert!(!result.status.success(), "no new task should be claimable");
    assert!(
        result.stderr.contains("no tasks are ready"),
        "expected 'no tasks are ready'; got:\n{}",
        result.stderr
    );

    // Complete Task 1 so Task 2 becomes ready.
    let r = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert_success(&r);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    // Now next should pick Task 2.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);
    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "2", "after completing Task 1, next picks Task 2");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_workspace_repeated_to_completion() {
    let (ws, machine_path) = create_workspace(
        "next-ws-repeat",
        "# Rhei: Workspace Next\n",
        &[
            ("a.md", "### Task 1: First\n**State:** draft\n"),
            ("b.md", "### Task 2: Second\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Third\n**State:** draft\n**Prior:** Task 2\n"),
        ],
    );

    drive_to_completion_via_next(&ws, &machine_path);

    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    // Verify next fails now.
    let result = run_cli("next", &ws, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success());

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn next_with_task_flag_targets_specific() {
    let (dir, plan_path, machine_path) = setup_single_file("next-task-flag", INDEPENDENT_PLAN);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "2"]);
    assert_success(&result);

    // Task 2 should be advanced, Tasks 1 and 3 remain draft.
    assert_task_state(&plan_path, &machine_path, "2", "pending");
    assert_task_state(&plan_path, &machine_path, "1", "draft");
    assert_task_state(&plan_path, &machine_path, "3", "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_json_includes_subtasks() {
    let (dir, plan_path, machine_path) = setup_single_file("next-subtasks", SUBTASK_PLAN);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    let subtasks = json["subtasks"].as_array().expect("subtasks should be array");
    assert_eq!(subtasks.len(), 2, "should have 2 subtasks");
    assert_eq!(subtasks[0]["id"], "1.1");
    assert_eq!(subtasks[0]["title"], "First subtask");
    assert_eq!(subtasks[1]["id"], "1.2");
    assert_eq!(subtasks[1]["title"], "Second subtask");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_fails_when_all_completed() {
    let plan = r#"# Rhei: All Done

## Tasks

### Task 1: Done
**State:** completed
"#;

    let (dir, plan_path, machine_path) = setup_single_file("next-done", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "should fail when nothing to advance");
    assert!(
        result.stderr.contains("no tasks are ready"),
        "expected 'no tasks are ready'; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_does_not_allow_cancelled_prerequisite_to_unblock_dependents() {
    let plan = r#"# Rhei: Cancelled Dependency

## Tasks

### Task 1: Abandoned
**State:** cancelled

### Task 2: Still blocked
**State:** draft
**Prior:** Task 1
"#;

    let (dir, plan_path, machine_path) = setup_single_file("next-cancelled-dep", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "cancelled prerequisite should keep Task 2 blocked");
    assert!(
        result.stderr.contains("no tasks are ready"),
        "expected 'no tasks are ready'; got:\n{}",
        result.stderr
    );

    let targeted = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "2"]);
    assert!(!targeted.status.success(), "targeted next should still respect blocked prerequisites");
    assert!(
        targeted.stderr.contains("blocked by incomplete prerequisites"),
        "expected blocked-prerequisite error; got:\n{}",
        targeted.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_fails_when_only_cancelled_terminal_is_available() {
    let plan = r#"# Rhei: Cancel Is Not Complete

## Tasks

### Task 1: Work item
**State:** active
"#;
    let machine = r#"name: cancelled-only
version: 1
states:
  active:
    description: Working
  cancelled:
    description: Abandoned
    final: true
transitions:
  - from: active
    to: cancelled
"#;

    let dir = unique_temp_dir("complete-no-cancel");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert!(!result.status.success(), "complete should fail instead of cancelling");
    assert!(
        result.stderr.contains("no transition to a terminal state available"),
        "expected missing-completion-target error; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "active");
    assert!(
        !dir.join("runtime/results/1.md").exists(),
        "result file should not be written on failure"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
