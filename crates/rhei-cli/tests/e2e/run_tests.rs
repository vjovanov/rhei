use std::fs;

use super::*;

#[test]
fn run_single_file_linear_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("run-linear", LINEAR_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions (2 per task); got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("3/3 tasks in terminal state"),
        "expected all tasks terminal; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_single_file_parallel_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("run-parallel", PARALLEL_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_single_file_independent_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("run-independent", INDEPENDENT_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_workspace_linear_to_completion() {
    let (ws, machine_path) = create_workspace(
        "run-ws-linear",
        "# Rhei: Workspace Linear\n",
        &[
            ("a.md", "### Task 1: First\n**State:** draft\n"),
            ("b.md", "### Task 2: Second\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Third\n**State:** draft\n**Prior:** Task 2\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    // Verify via CLI render.
    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    // Verify individual task files contain the updated state.
    for name in &["a.md", "b.md", "c.md"] {
        let content = fs::read_to_string(ws.join("tasks").join(name)).expect("read task file");
        assert!(
            content.contains("**State:** completed"),
            "{} should contain completed state: {}",
            name,
            content
        );
    }

    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn run_workspace_parallel_to_completion() {
    let (ws, machine_path) = create_workspace(
        "run-ws-parallel",
        "# Rhei: Workspace Parallel\n",
        &[
            ("a.md", "### Task 1: Root\n**State:** draft\n"),
            ("b.md", "### Task 2: Branch A\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Branch B\n**State:** draft\n**Prior:** Task 1\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn run_partially_advanced_completes_remaining() {
    let plan = r#"# Rhei: Partial Advance

## Tasks

### Task 1: Already done
**State:** completed

### Task 2: Needs work
**State:** draft
**Prior:** Task 1

### Task 3: Also needs work
**State:** draft
**Prior:** Task 2
"#;

    let (dir, plan_path, machine_path) = setup_single_file("run-partial", plan);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("4 transition(s) made"),
        "expected 4 transitions (2 each for Tasks 2 & 3); got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_already_completed_is_noop() {
    let plan = r#"# Rhei: All Done

## Tasks

### Task 1: Done
**State:** completed

### Task 2: Also done
**State:** completed
**Prior:** Task 1
"#;

    let (dir, plan_path, machine_path) = setup_single_file("run-noop", plan);
    let original = fs::read_to_string(&plan_path).expect("read plan");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "expected no-op message; got:\n{}",
        result.stdout
    );

    let after = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(original, after, "file should be unchanged");

    fs::remove_dir_all(dir).expect("cleanup");
}
