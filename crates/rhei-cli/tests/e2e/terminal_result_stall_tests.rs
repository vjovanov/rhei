//! A worker that exits 0 owing the ticket's result, and what the run says
//! about it.
//!
//! The report has to name the artifact that is missing rather than fall back to
//! "stalled in non-terminal state", and one stalled ticket must not take its
//! siblings — or the rest of the run — down with it.

// §FS-rhei-states.3.3 §FS-rhei-run-report.3.1 §FS-rhei-run.3

use std::fs;

use super::terminal_result_fanout_tests::{write_fanout_agent_settings, FANOUT_PLAN};
use super::terminal_result_tests::{
    write_exiting_agent, write_mock_agent_settings, write_silent_agent,
};
use super::*;

/// `rhei run` spawns a program once per ticket, whatever the state declares, so
/// a program state writes the ticket's result file and is never asked for a
/// fragment per declared target — files nothing could write.
// §FS-rhei-states.3.3 §FS-rhei-programs.2
fn program_fanout_machine(command: &str) -> String {
    format!(
        r#"name: program-fanout
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: One program, many declared targets
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    program:
      command: {command}
    program_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: review
    to: completed
    exit_code: 0
"#
    )
}

#[test]
fn a_program_state_with_declared_targets_writes_the_ticket_result() {
    let dir = unique_temp_dir("terminal-result-program-fanout");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let program = write_python_agent(
        &dir,
        "the-program.py",
        r#"result('the program did it.\n')
"#,
    );
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &program_fanout_machine(&fixture_command(&program)),
    );
    let agent = write_silent_agent(&dir);
    write_fanout_agent_settings(&dir, &agent);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    let result =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("ticket result file");
    assert!(result.contains("the program did it."), "got:\n{result}");
    assert!(
        !dir.join("runtime/results/plan.1").exists(),
        "a program state files no per-invocation fragments"
    );
}

/// A program is a worker: when it exits 0 owing the ticket's result, the run
/// report must name the file, not fall back to "stalled in non-terminal state".
// §FS-rhei-run-report.3.1 §FS-rhei-agents.3.2.1
fn silent_program_machine(command: &str) -> String {
    format!(
        r#"name: silent-program
version: 1
states:
  probe:
    initial: true
    description: Exits 0 and writes nothing
    program:
      command: {command}
    program_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: probe
    to: completed
    exit_code: 0
"#
    )
}

const SILENT_PROGRAM_PLAN: &str = r#"# Rhei: Silent Program

## Tasks

### Task 1: Probe
**State:** probe
"#;

#[test]
fn a_program_that_owes_the_result_is_reported_as_missing_outputs() {
    for parallel in ["1", "2"] {
        let dir = unique_temp_dir(&format!("terminal-result-program-stall-{parallel}"));
        let plan_path = write_fixture_file(&dir, "plan.rhei.md", SILENT_PROGRAM_PLAN);
        let probe = write_exiting_agent(&dir, "probe.py", 0);
        let machine_path = write_fixture_file(
            &dir,
            "states.yaml",
            &silent_program_machine(&fixture_command(&probe)),
        );

        let result = run_cli(
            "run",
            &plan_path,
            &machine_path,
            &["--no-tui", "--no-callbacks", "--parallel", parallel],
        );
        assert!(!result.status.success(), "--parallel {parallel}: the run halts on the stall");
        let combined = format!("{}{}", result.stdout, result.stderr);
        assert!(
            combined.contains("program exited 0 but required outputs are missing"),
            "--parallel {parallel}: the console still says `program`; got:\n{combined}"
        );
        let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
        assert!(
            report.contains("worker exited 0 without result ("),
            "--parallel {parallel}: the report names the artifact the program owed; got:\n{report}"
        );
        assert!(
            !report.contains("stalled in non-terminal state"),
            "--parallel {parallel}: not the nameless stall; got:\n{report}"
        );
        assert!(
            report.contains(&plan_path.display().to_string())
                && report.contains("--task plan.1 --from probe"),
            "--parallel {parallel}: the suggested command carries the plan; got:\n{report}"
        );
    }
}

/// Sequential mode is the default, and a stall there used to end the whole run:
/// the pass broke out of the loop, so a healthy ticket mid-workflow never got
/// its next state and no second pass happened.
// §FS-rhei-run.3 §FS-rhei-agents.5.2.1
fn sequential_stall_machine(command: &str) -> String {
    format!(
        r#"name: sequential-stall
version: 1
states:
  probe:
    initial: true
    description: Advances the ticket into work
    program:
      command: {command}
    program_timeout: 20s
  work:
    description: Agent work
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: probe
    to: work
    exit_code: 0
  - from: work
    to: completed
"#
    )
}

const SEQUENTIAL_STALL_PLAN: &str = r#"# Rhei: Sequential Stall

## Tasks

### Task 1: Probe then work
**State:** probe

### Task 2: Silent worker
**State:** work
"#;

#[test]
fn a_sequential_stall_does_not_end_the_run() {
    let dir = unique_temp_dir("terminal-result-sequential-stall");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", SEQUENTIAL_STALL_PLAN);
    let probe = write_exiting_agent(&dir, "probe.py", 0);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &sequential_stall_machine(&fixture_command(&probe)),
    );
    // Task 2 is the silent one; task 1 writes its result and finishes.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"task = env('RHEI_TASK_ID')
if task != 'plan.2':
    result('{} finished.\n'.format(task))
"#,
    );
    write_mock_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!result.status.success(), "the run still halts on the ticket that stalled");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("Pass 2"), "the pass after the stall happens; got:\n{combined}");
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_task_state(&plan_path, &machine_path, "2", "work");
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("| plan.2 | work | worker exited 0 without result ("),
        "the stalled ticket is reported by the artifact it owes; got:\n{report}"
    );
}

const THREE_WORKER_PLAN: &str = r#"# Rhei: Three Workers

## Tasks

### Task 1: Worker one
**State:** work

### Task 2: Silent worker
**State:** work

### Task 3: Worker three
**State:** work
"#;

/// One ticket stalling must not take its siblings down with it, including the
/// ones a non-concurrent state deferred behind it.
// §FS-rhei-run.3
#[test]
fn a_sequential_stall_leaves_its_siblings_claimable() {
    let dir = unique_temp_dir("terminal-result-sequential-siblings");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", THREE_WORKER_PLAN);
    let probe = write_exiting_agent(&dir, "probe.py", 0);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &sequential_stall_machine(&fixture_command(&probe)),
    );
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"task = env('RHEI_TASK_ID')
if task != 'plan.2':
    result('{} finished.\n'.format(task))
"#,
    );
    write_mock_agent_settings(&dir, &agent);

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--parallel", "1"],
    );
    assert!(!result.status.success(), "the silent ticket still halts the run");
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_task_state(&plan_path, &machine_path, "2", "work");
    assert_task_state(&plan_path, &machine_path, "3", "completed");
}
