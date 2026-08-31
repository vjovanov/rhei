// §FS-rhei-agents.3.2.1: a poll state's re-spawn note names `poll.max_attempts`,
// not the internal sentinel that marks it exempt from the visit budget.

#[test]
fn run_poll_respawn_note_names_poll_max_attempts_not_the_sentinel() {
    let dir = unique_temp_dir("run-poll-respawn-budget");
    let script = write_python_agent(
        &dir,
        "poll.py",
        r#"append(pathlib.Path('runtime') / 'attempts.txt', 'attempt\n')
sys.exit(75)
"#,
    );
    let machine = format!(
        r#"name: run-poll-respawn-budget-test
version: 1
states:
  waiting:
    description: Poll until attempts are exhausted
    program:
      command: {command}
    poll:
      interval: 0s
      max_attempts: 3
  exhausted:
    description: Polling exhausted
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: exhausted
    exit_code: 75
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Poll Respawn Budget

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "poll run should route to exhaustion after three attempts\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("attempt 2 of 3 (poll.max_attempts)"),
        "respawn note should name poll.max_attempts\nstdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("attempt 3 of 3 (poll.max_attempts)"),
        "respawn note should name poll.max_attempts\nstdout:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("18446744073709551615"),
        "respawn note must never print the internal exempt sentinel\nstdout:\n{}",
        result.stdout
    );
}

// §FS-rhei-run.5.1: the typical exhaustion edge is `condition:` only, no
// `exit_code` — unlike the sibling test above, whose two transitions both
// declare `exit_code: 75` and so miss a selector dropping exit_code-less rules.
#[test]
fn run_poll_exhaustion_routes_a_condition_only_transition_on_nonzero_exit() {
    let dir = unique_temp_dir("run-poll-exhaustion-condition-only");
    let script = write_python_agent(
        &dir,
        "poll.py",
        r#"append(pathlib.Path('runtime') / 'attempts.txt', 'attempt\n')
sys.exit(75)
"#,
    );
    let machine = format!(
        r#"name: run-poll-exhaustion-condition-only-test
version: 1
states:
  waiting:
    description: Poll until attempts are exhausted
    program:
      command: {command}
    poll:
      interval: 0s
      max_attempts: 2
  exhausted:
    description: Polling exhausted
    final: true
transitions:
  - from: waiting
    to: waiting
    exit_code: 75
  - from: waiting
    to: exhausted
    condition: pollAttempts >= pollMaxAttempts
"#,
        command = fixture_command(&script)
    );
    let plan = r#"# Rhei: Poll Exhaustion Condition Only

## Tasks

### Task 1: Wait for external status
**State:** waiting
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);
    assert!(
        result.status.success(),
        "an exit_code-less exhaustion transition must still route once the poll budget is spent\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let attempts = fs::read_to_string(dir.join("runtime/attempts.txt"))
        .expect("the program should have run once per attempt");
    assert_eq!(
        attempts.lines().count(),
        2,
        "should stop at max_attempts via the condition-only edge, not loop forever or abort early\n{}",
        attempts
    );
}
