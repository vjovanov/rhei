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
**Assignee:** codex

#### Task 1.1: Detail
**State:** in-progress
**Assignee:** claude-code

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
    assert_eq!(rhei.tasks[0].assignee, None);
    assert_eq!(rhei.tasks[0].children[0].assignee, None);
    assert_eq!(rhei.tasks[1].assignee, None);
    assert!(!updated.contains("**Assignee:**"));
}

// ── Directory Workspace tests ────────────────────────────────────────────────

/// §FS-rhei-reset.1.2: without a terminal there is nobody to answer, so reset
/// refuses rather than reading consent into an unattended caller's silence.
#[test]
fn reset_refuses_without_yes_when_stdin_is_not_a_terminal() {
    let machine = r#"name: reset-noninteractive
version: 1
states:
  pending:
    description: Ready
    initial: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;

    let plan = r#"# Rhei: Unattended

## Tasks

### Task 1: Alpha
**State:** completed
"#;

    let dir = unique_temp_dir("reset-noninteractive");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command_with_args(&plan_path, &machine_path, &[]);

    assert!(
        !result.status.success(),
        "reset without -y on a pipe should fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("stdin is not a terminal"),
        "error should explain why it stopped:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("`-y`"),
        "error should name the flag that confirms:\n{}",
        result.stderr
    );

    let untouched = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        untouched.contains("**State:** completed"),
        "a refused reset must not rewrite state:\n{untouched}"
    );
}

/// §FS-rhei-reset.1.2: the damage preview precedes every destructive reset,
/// including the `--yes` path that never stops to ask.
#[test]
fn reset_prints_the_damage_preview_even_with_yes() {
    let machine = r#"name: reset-preview
version: 1
states:
  pending:
    description: Ready
    initial: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#;

    let plan = r#"# Rhei: Previewed

## Tasks

### Task 1: Alpha
**State:** completed
"#;

    let dir = unique_temp_dir("reset-preview");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    fs::create_dir_all(dir.join("runtime/results")).expect("seed runtime tree");
    fs::write(dir.join("runtime/results/1.md"), "## Result\n\ndone\n").expect("seed result");

    let result = run_reset_command_with_args(&plan_path, &machine_path, &["-y"]);

    assert!(
        result.status.success(),
        "reset -y should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Would reset"),
        "preview must print before the destructive step:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Would delete"),
        "preview must name the runtime tree it deletes:\n{}",
        result.stdout
    );
}
