use std::fs;

use super::*;

#[test]
fn transition_single_file_full_advancement() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-full", INDEPENDENT_PLAN);

    // Advance all 3 tasks: draft -> pending -> completed.
    for task_id in &["1", "2", "3"] {
        let r = run_transition(&plan_path, &machine_path, task_id, "draft", "pending");
        assert_success(&r);
        let r = run_transition(&plan_path, &machine_path, task_id, "pending", "completed");
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
        let r = run_transition(&ws, &machine_path, task_id, "pending", "completed");
        assert_success(&r);
    }

    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn transition_wildcard_to_cancelled() {
    let (dir, plan_path, machine_path) = setup_single_file("trans-wildcard", INDEPENDENT_PLAN);

    let result = run_transition(&plan_path, &machine_path, "1", "draft", "cancelled");
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
