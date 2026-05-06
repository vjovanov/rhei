use std::fs;

use super::*;

#[test]
fn next_auto_discovers_sibling_state_machine_from_states_declaration() {
    let plan = r#"# Rhei: Auto-discovered Machine
**States:** custom-review

## Tasks

### Task 1: Review API surface
**State:** draft
Inspect public interfaces.
"#;
    let machine = r#"name: custom-review
version: 1
states:
  draft:
    initial: true
    description: Planned
  review:
    description: Review in progress
    instructions: Follow the custom review workflow.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: review
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("next-auto-states");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli_without_machine("next", &plan_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["task_id"], "1");
    assert_eq!(json["state"], "review");
    assert_eq!(json["from_state"], "draft");
    assert!(
        json["instructions"]
            .as_str()
            .expect("instructions string")
            .contains("Follow the custom review workflow."),
        "expected custom machine instructions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

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
    assert!(json["children"].is_array());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_prints_state_personality_in_text_and_json_when_configured() {
    let plan = r#"# Rhei: Personality Output Test

## Tasks

### Task 1: Teach concurrency
**State:** draft
Explain lock-free tradeoffs.
"#;
    let machine = r#"name: professor-demo
version: 1
states:
  draft:
    initial: true
    description: Planning
    instructions: Analyze first.
  pending:
    description: Ready
    personality: You are an MIT professor.
    instructions: Teach clearly.
  done:
    description: Done
    final: true
transitions:
  - from: draft
    to: pending
  - from: pending
    to: done
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
fn next_resolves_runtime_template_variables_in_instructions() {
    let plan = r#"# Rhei: Template Resolution

## Tasks

### Task 1: Review cache layer
**State:** draft
Check the implementation carefully.
"#;
    let machine = r#"name: template-demo
version: 1
states:
  draft:
    initial: true
    description: Planned
  review:
    description: Review work
    visits: 2
    instructions: |
      Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Plan: {plan_title}
      Write findings to {output.findings.path}.
    outputs:
      - name: findings
        path: runtime/findings/{task_id}-{visit_count}.md
  done:
    description: Done
    final: true
transitions:
  - from: draft
    to: review
  - from: review
    to: done
"#;

    let dir = unique_temp_dir("next-template-vars");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert_success(&result);
    assert!(
        result.stdout.contains("Review pass 1 of 2 for Task 1: Review cache layer."),
        "expected visit/task placeholders to resolve; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Plan: Template Resolution"),
        "expected plan_title placeholder to resolve; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("runtime/findings/1-1.md"),
        "expected output artifact path placeholder to resolve; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_does_not_auto_transition_runnable_initial_states() {
    let plan = r#"# Rhei: Runnable Initial State

## Tasks

### Task coordinate: Coordinate review
**State:** split
Review the change and write the manifest.
"#;
    let machine = r#"name: runnable-initial
version: 1
models:
  - codex
states:
  split:
    initial: true
    description: Coordinator
    model: codex
    instructions: |
      Write the overview to `{output.overview.path}`.
    outputs:
      - name: overview
        path: runtime/manifests/coordinate.md
  completed:
    final: true
    description: Done
transitions:
  - from: split
    to: completed
"#;

    let dir = unique_temp_dir("next-runnable-initial");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["task_id"], "coordinate");
    assert_eq!(json["from_state"], "split");
    assert_eq!(json["state"], "split");
    assert!(
        json["instructions"]
            .as_str()
            .expect("instructions string")
            .contains("runtime/manifests/coordinate.md"),
        "expected runnable-state instructions; got:\n{}",
        result.stdout
    );
    assert_task_state(&plan_path, &machine_path, "coordinate", "split");

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
        result.stderr.contains("no tasks can be auto-claimed")
            || result.stderr.contains("no tasks are ready"),
        "expected no-claimable diagnostic; got:\n{}",
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
fn next_json_includes_children() {
    let (dir, plan_path, machine_path) = setup_single_file("next-children", SUBTASK_PLAN);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    let children = json["children"].as_array().expect("children should be array");
    assert_eq!(children.len(), 2, "should have 2 child tasks");
    assert_eq!(children[0]["id"], "1.1");
    assert_eq!(children[0]["title"], "First subtask");
    assert_eq!(children[1]["id"], "1.2");
    assert_eq!(children[1]["title"], "Second subtask");

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

#[test]
fn next_fails_with_explicit_error_when_current_state_input_artifact_is_missing() {
    let plan = r#"# Rhei: Missing Current Input

## Tasks

### Task 1: Apply findings
**State:** fix
"#;
    let machine = r#"name: missing-current-input
version: 1
states:
  draft:
    description: Planned
    initial: true
  fix:
    description: Needs findings
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: fix
  - from: fix
    to: completed
"#;

    let dir = unique_temp_dir("next-missing-input");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert!(!result.status.success(), "next should fail when current-state input is missing");
    assert!(
        result.stderr.contains("Task 1 cannot be claimed in state fix."),
        "expected explicit claim failure; got:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("Missing required input artifact: findings (runtime/findings/1.md)"),
        "expected missing artifact detail; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "fix");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_fails_when_required_output_artifact_is_missing() {
    let plan = r#"# Rhei: Missing Completion Output

## Tasks

### Task 1: Review item
**State:** review
"#;
    let machine = r#"name: missing-output
version: 1
states:
  review:
    description: Must produce findings before leaving
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
transitions:
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("complete-missing-output");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert!(!result.status.success(), "complete should fail when required output is missing");
    assert!(
        result.stderr.contains("Task 1 cannot leave state review."),
        "expected explicit leave failure; got:\n{}",
        result.stderr
    );
    assert!(
        result
            .stderr
            .contains("Missing required output artifact: findings (runtime/findings/1.md)"),
        "expected missing artifact detail; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "review");
    assert!(
        !dir.join("runtime/results/1.md").exists(),
        "result file should not be written on failure"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
