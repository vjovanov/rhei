#[test]
fn reset_restores_single_file_plan_to_initial_state() {
    let machine = r#"name: reset-test
version: 1
states:
  draft:
    description: Start here
    initial: true
  pending:
    description: Ready
  in-progress:
    description: Active
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: pending
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: Resettable

## Tasks

### Task 1: Alpha
**State:** completed

#### Task 1.1: Detail
**State:** in-progress

### Task 2: Beta
**State:** pending
"#;

    let dir = unique_temp_dir("reset-single-file");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command(&plan_path, &machine_path);

    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result
            .stdout
            .contains("Reset 2 task(s) (and 1 descendant task(s)) to initial state 'draft'."),
        "unexpected stdout:\n{}",
        result.stdout
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse reset plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "draft");
    assert_eq!(rhei.tasks[0].children[0].state.as_str(), "draft");
    assert_eq!(rhei.tasks[1].state.as_str(), "draft");

    fs::remove_dir_all(dir).expect("cleanup");
}

// ── Directory Workspace tests ────────────────────────────────────────────────
