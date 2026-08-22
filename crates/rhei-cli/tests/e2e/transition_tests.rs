use std::fs;

use super::*;

#[test]
fn transition_single_file_full_advancement() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-full", INDEPENDENT_PLAN);

    // Advance all 3 tasks: draft -> pending -> completed. The terminal hop
    // carries the result every `final: true` entry requires. §FS-rhei-states.3.3
    for task_id in &["1", "2", "3"] {
        let r = run_transition(&plan_path, &machine_path, task_id, "draft", "pending");
        assert_success(&r);
        let r = run_transition_with_result(
            &plan_path,
            &machine_path,
            task_id,
            "pending",
            "completed",
            "Done by hand.",
        );
        assert_success(&r);
    }

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_cas_rejects_wrong_from() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-cas-wrong", INDEPENDENT_PLAN);

    // Task 1 is in draft, but we claim it's pending.
    let result = run_transition(&plan_path, &machine_path, "1", "pending", "completed");
    assert!(!result.status.success(), "should fail on CAS conflict");
    assert!(result.stderr.contains("conflict"), "should report conflict; got:\n{}", result.stderr);
    assert!(
        result.stderr.contains("draft"),
        "should mention actual state 'draft'; got:\n{}",
        result.stderr
    );

    // File unchanged.
    assert_task_state(&plan_path, &machine_path, "1", "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_cas_rejects_after_concurrent_change() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-cas-stale", INDEPENDENT_PLAN);

    // First transition succeeds.
    let r = run_transition(&plan_path, &machine_path, "1", "draft", "pending");
    assert_success(&r);
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    // Second transition with stale --from draft fails.
    let r = run_transition(&plan_path, &machine_path, "1", "draft", "pending");
    assert!(!r.status.success(), "stale CAS should fail");
    assert!(r.stderr.contains("conflict"), "should report conflict; got:\n{}", r.stderr);

    // Task stays at pending.
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_workspace_updates_correct_file() {
    let (ws, machine_path) = create_workspace(
        "trans-ws-correct",
        "# Rhei: Workspace Transition\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** draft\n"),
            ("b.md", "### Task 2: Beta\n**State:** draft\n"),
            ("c.md", "### Task 3: Gamma\n**State:** draft\n"),
        ],
    );

    let result = run_transition(&ws, &machine_path, "2", "draft", "pending");
    assert_success(&result);

    // Only b.md should be modified.
    let b = fs::read_to_string(ws.join("tasks/b.md")).expect("read b.md");
    assert!(b.contains("**State:** pending"), "b.md should be updated: {}", b);

    // a.md and c.md untouched.
    let a = fs::read_to_string(ws.join("tasks/a.md")).expect("read a.md");
    assert!(a.contains("**State:** draft"), "a.md should be untouched: {}", a);
    let c = fs::read_to_string(ws.join("tasks/c.md")).expect("read c.md");
    assert!(c.contains("**State:** draft"), "c.md should be untouched: {}", c);

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn transition_workspace_full_advancement() {
    let (ws, machine_path) = create_workspace(
        "trans-ws-full",
        "# Rhei: Workspace Full\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** draft\n"),
            ("b.md", "### Task 2: Beta\n**State:** draft\n"),
        ],
    );

    for task_id in &["1", "2"] {
        let r = run_transition(&ws, &machine_path, task_id, "draft", "pending");
        assert_success(&r);
        let r = run_transition_with_result(
            &ws,
            &machine_path,
            task_id,
            "pending",
            "completed",
            "Done by hand.",
        );
        assert_success(&r);
    }

    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn transition_wildcard_to_cancelled() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-wildcard", INDEPENDENT_PLAN);

    let result = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "draft",
        "cancelled",
        "Abandoned: superseded by Task 2.",
    );
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "cancelled");
    // Other tasks unaffected.
    assert_task_state(&plan_path, &machine_path, "2", "draft");
    assert_task_state(&plan_path, &machine_path, "3", "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_disallowed_path_rejected() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-disallowed", INDEPENDENT_PLAN);

    // draft -> completed is not a declared transition.
    let result = run_transition(&plan_path, &machine_path, "1", "draft", "completed");
    assert!(!result.status.success(), "disallowed transition should fail");
    assert!(
        result.stderr.contains("not allowed"),
        "should report 'not allowed'; got:\n{}",
        result.stderr
    );

    // File unchanged.
    assert_task_state(&plan_path, &machine_path, "1", "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_fails_when_target_state_input_artifact_is_missing() {
    let plan = r#"# Rhei: Missing Target Input

## Tasks

### Task 1: Review item
**State:** draft
"#;
    let machine = r#"name: artifact-input
version: 1
states:
  draft:
    description: Planned
    initial: true
  review:
    description: Needs an input artifact
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: review
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("trans-missing-input");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_transition(&plan_path, &machine_path, "1", "draft", "review");
    assert!(!result.status.success(), "transition should fail when target input is missing");
    // §FS-rhei-panta.6: artifact paths render the project-qualified id.
    assert_stderr_contains(&result, "Task plan.1 cannot enter state review.");
    assert_stderr_contains(
        &result,
        "Missing required input artifact: findings (runtime/findings/plan.1.md)",
    );
    assert_task_state(&plan_path, &machine_path, "1", "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

const PARENT_TRANSITION_PLAN: &str = r#"# Rhei: Descendants First

## Tasks

### Task 1: Parent task
**State:** pending

#### Task 1.1: Open subtask
**State:** pending
"#;

/// The descendants-first guard lives on the shared transition path, so
/// `rhei transition` — the escape hatch that deliberately skips `**Prior:**`
/// readiness — still cannot produce the plan `rhei validate` calls an error.
// §FS-rhei-transition-cmd.3.1
#[test]
fn transition_rejects_terminal_entry_on_a_parent_with_an_open_descendant() {
    let dir = unique_temp_dir("trans-open-descendant");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PARENT_TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "completed");
    assert!(
        !result.status.success(),
        "transition into a terminal state must be refused\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(
        &result,
        "Task plan.1 cannot enter terminal state 'completed' while descendant tasks remain \
         non-terminal.",
    );
    // Same `Task <id> (<state>)` shape `rhei next` and the run report use.
    // §FS-rhei-transition-cmd.3.1
    assert_stderr_contains(&result, "Task plan.1.1 (pending)");
    // The refusal names what to run to find the open work.
    assert_stderr_contains(&result, "--non-terminal");
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The declared-edge check comes first: an edge the machine never declared is
/// refused as such, whether or not the task happens to be a parent. Reporting
/// "close your subtree" for a move the machine forbids outright sent a user off
/// to finish work that would not have unblocked anything.
// §FS-rhei-transition-cmd.3
#[test]
fn transition_reports_an_undeclared_edge_before_the_descendants_guard() {
    let plan = r#"# Rhei: Undeclared Terminal Edge

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Open subtask
**State:** draft
"#;
    let dir = unique_temp_dir("trans-undeclared-descendant");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    // `draft -> completed` is not a declared edge, and Task 1 also has an open
    // child: the state machine's answer is the one that matters.
    let result = run_transition(&plan_path, &machine_path, "1", "draft", "completed");
    assert!(!result.status.success(), "an undeclared edge must be refused");
    assert_stderr_contains(
        &result,
        "transition from 'draft' to 'completed' is not allowed by the state machine",
    );
    assert!(
        !result.stderr.contains("descendant tasks remain non-terminal"),
        "the descendants guard must not speak for an edge that does not exist; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// And the edge's own `condition:` comes first for the same reason: an edge
/// that is declared but not currently applicable is not a move the user could
/// have made by closing the subtree, so the guard must not claim the subtree is
/// what stands in the way.
// §FS-rhei-transition-cmd.3
#[test]
fn transition_reports_an_inapplicable_edge_before_the_descendants_guard() {
    let plan = r#"# Rhei: Conditional Terminal Edge

## Tasks

### Task 1: Parent task
**State:** fix

#### Task 1.1: Open subtask
**State:** draft
"#;
    let machine = r#"name: conditional-terminal-edge
version: 1
states:
  draft:
    initial: true
    description: Draft
  fix:
    description: Fix findings
    visits: 2
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: fix
  - from: fix
    to: fix
    condition: visitCount < visits
  - from: fix
    to: completed
    condition: visitCount >= visits
"#;
    let dir = unique_temp_dir("trans-inapplicable-descendant");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // `fix -> completed` is declared but its condition is unmet on visit 1, and
    // Task 1 also has an open child: the condition is the honest answer.
    let result = run_transition(&plan_path, &machine_path, "1", "fix", "completed");
    assert!(!result.status.success(), "an inapplicable edge must be refused");
    assert_stderr_contains(
        &result,
        "transition from 'fix' to 'completed' is not currently applicable",
    );
    assert!(
        !result.stderr.contains("descendant tasks remain non-terminal"),
        "the descendants guard must not speak for an edge that is not applicable; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "fix");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Cancellation is a terminal entry too, so abandoning a parent while its
/// subtree is open is refused on the same edge.
// §FS-rhei-transition-cmd.3.1
#[test]
fn transition_rejects_cancelling_a_parent_with_an_open_descendant() {
    let dir = unique_temp_dir("trans-cancel-descendant");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PARENT_TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "cancelled");
    assert!(!result.status.success(), "cancelling a parent with an open child must be refused");
    assert_stderr_contains(&result, "cannot enter terminal state 'cancelled'");
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Same edge, same command, once the subtree is closed: the guard is about
/// open descendants, not about being a parent. A cancelled descendant is
/// terminal and closes the subtree just as a completed one does.
// §FS-rhei-transition-cmd.3.1
#[test]
fn transition_allows_terminal_entry_once_the_subtree_is_closed() {
    let dir = unique_temp_dir("trans-closed-descendant");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PARENT_TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    assert_success(&run_transition_with_result(
        &plan_path,
        &machine_path,
        "1.1",
        "pending",
        "cancelled",
        "Abandoned.",
    ));
    assert_success(&run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "pending",
        "completed",
        "Integrated the subtree.",
    ));
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A cancel does not owe the abandoned step's `outputs:`.
///
/// Cancellation abandons the work, so the source state's artifact contract is
/// moot — requiring it made a step whose state declares an output impossible
/// to drop, which is exactly the step a supervisor wants to drop.
// §FS-rhei-transitions.4.5 §FS-rhei-supervision.6
#[test]
fn cancelling_waives_the_source_states_outputs_but_not_its_result() {
    let plan = r#"# Rhei: Cancel

## Tasks

### Task 1: Review item
**State:** review
"#;
    let machine = r#"name: cancel-waiver
version: 1
states:
  review:
    description: Must produce findings before finishing
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: review, to: completed, description: Reviewed }
  - { from: review, to: cancelled, description: Dropped }
"#;
    let dir = unique_temp_dir("trans-cancel-waiver");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // The finishing edge still owes the output.
    let finished = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "review",
        "completed",
        "Reviewed.",
    );
    assert!(!finished.status.success(), "finishing still owes the declared output");
    assert_stderr_contains(&finished, "Missing required output artifact: findings");

    // The cancel does not — but it still owes a result.
    let silent = run_transition(&plan_path, &machine_path, "1", "review", "cancelled");
    assert!(!silent.status.success(), "a cancelled ticket still has to say why");
    assert_stderr_contains(&silent, "cannot enter terminal state 'cancelled' without a result");

    let cancelled = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "review",
        "cancelled",
        "Made unnecessary.",
    );
    assert_success(&cancelled);
    assert_task_state(&plan_path, &machine_path, "1", "cancelled");

    fs::remove_dir_all(dir).expect("cleanup");
}
