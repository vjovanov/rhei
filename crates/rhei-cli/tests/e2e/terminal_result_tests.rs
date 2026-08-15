//! A `final: true` state is not entered without a result, whichever verb drove
//! the edge. §FS-rhei-states.3.3 §FS-rhei-transition-cmd.3.2

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const RESULT_MESSAGE: &str = "Added avatar_url column and migration 0042";

/// One machine for all three drivers, so the "same edge" in these tests really
/// is the same edge: `pending` declares an agent so `rhei run` can work it, and
/// a manual worker can finish the same ticket by hand.
const AGENT_TERMINAL_MACHINE: &str = r#"name: terminal-result
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    agent: mock
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

const ONE_TASK_PLAN: &str = r#"# Rhei: Terminal Result

## Tasks

### Task 1: Do the work
**State:** pending
**Assignee:** worker-1
"#;

/// A mock agent that writes `body` to the path Rhei told it to use. Passing the
/// path in `RHEI_RESULT_PATH` is the contract a program has instead of a
/// prompt. §FS-rhei-agents.4
fn write_result_writing_agent(dir: &Path, body: &str) -> PathBuf {
    write_fixture_file(
        dir,
        "mock-agent.sh",
        &format!(
            r#"#!/bin/sh
set -eu
mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
cat > "$RHEI_RESULT_PATH" <<'RESULT_EOF'
{body}
RESULT_EOF
"#
        ),
    )
}

fn write_silent_agent(dir: &Path) -> PathBuf {
    write_fixture_file(dir, "mock-agent.sh", "#!/bin/sh\nset -eu\nexit 0\n")
}

fn write_mock_agent_settings(workspace_root: &Path, script: &Path) {
    let settings_dir = workspace_root.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{
    "mock": {{ "command": ["sh", {script_json}], "timeout": "10s" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
}

/// The four artifacts a terminal entry leaves behind, read back from disk.
struct TerminalTrail {
    ledger: String,
    result: String,
    plan: String,
}

fn read_terminal_trail(dir: &Path, plan_path: &Path) -> TerminalTrail {
    TerminalTrail {
        ledger: fs::read_to_string(dir.join("runtime/state-transitions.log"))
            .expect("read transition ledger"),
        result: fs::read_to_string(dir.join("runtime/results/plan.1.md"))
            .expect("read result file"),
        plan: fs::read_to_string(plan_path).expect("read plan"),
    }
}

fn assert_finished_trail(trail: &TerminalTrail, expected_result: &str, driver: &str) {
    assert_eq!(trail.ledger.trim(), "plan.1 pending@completed", "{driver}: ledger line differs");
    assert_eq!(trail.result.trim(), expected_result, "{driver}: result file differs");
    assert!(
        trail.plan.contains("> **Result:** [plan.1](runtime/results/plan.1.md)"),
        "{driver}: task body should link the result; got:\n{}",
        trail.plan
    );
    assert!(
        !trail.plan.contains("**Assignee:**"),
        "{driver}: terminal entry drops the assignee; got:\n{}",
        trail.plan
    );
    assert!(
        trail.plan.contains("**State:** completed"),
        "{driver}: task should be completed; got:\n{}",
        trail.plan
    );
}

fn setup_terminal_result_case(prefix: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", AGENT_TERMINAL_MACHINE);
    (dir, plan_path, machine_path)
}

/// The test of done: `rhei complete --result`, `rhei transition --result`, and
/// `rhei run` (whose agent wrote the result file) all take the same edge and
/// leave the same ledger line, the same result file, the same `> **Result:**`
/// link, and no `**Assignee:**`.
// §FS-rhei-complete.4 §FS-rhei-states.3.3
#[test]
fn every_verb_leaves_the_same_terminal_trail() {
    let expected = format!("## Result\n\n{RESULT_MESSAGE}");

    let (complete_dir, complete_plan, complete_machine) =
        setup_terminal_result_case("terminal-result-complete");
    write_mock_agent_settings(&complete_dir, &write_silent_agent(&complete_dir));
    assert_success(&run_cli(
        "complete",
        &complete_plan,
        &complete_machine,
        &["--task", "1", "--result", RESULT_MESSAGE, "--no-callbacks"],
    ));
    let by_complete = read_terminal_trail(&complete_dir, &complete_plan);

    let (transition_dir, transition_plan, transition_machine) =
        setup_terminal_result_case("terminal-result-transition");
    write_mock_agent_settings(&transition_dir, &write_silent_agent(&transition_dir));
    assert_success(&run_cli(
        "transition",
        &transition_plan,
        &transition_machine,
        &[
            "--task",
            "1",
            "--from",
            "pending",
            "--to",
            "completed",
            "--result",
            RESULT_MESSAGE,
            "--no-callbacks",
        ],
    ));
    let by_transition = read_terminal_trail(&transition_dir, &transition_plan);

    let (run_dir, run_plan, run_machine) = setup_terminal_result_case("terminal-result-run");
    let agent = write_result_writing_agent(&run_dir, &expected);
    write_mock_agent_settings(&run_dir, &agent);
    // The run does not steal a claimed ticket, so this one is worked unassigned.
    fs::write(&run_plan, ONE_TASK_PLAN.replace("**Assignee:** worker-1\n", ""))
        .expect("drop assignee for the run case");
    assert_success(&run_cli("run", &run_plan, &run_machine, &["--no-tui", "--no-callbacks"]));
    let by_run = read_terminal_trail(&run_dir, &run_plan);

    assert_finished_trail(&by_complete, &expected, "complete");
    assert_finished_trail(&by_transition, &expected, "transition");
    assert_finished_trail(&by_run, &expected, "run");
    assert_eq!(by_complete.ledger, by_transition.ledger);
    assert_eq!(by_complete.ledger, by_run.ledger);
    assert_eq!(by_complete.result, by_transition.result);
    assert_eq!(by_complete.result.trim(), by_run.result.trim());

    fs::remove_dir_all(complete_dir).expect("cleanup");
    fs::remove_dir_all(transition_dir).expect("cleanup");
    fs::remove_dir_all(run_dir).expect("cleanup");
}

/// The refusal comes before the state write, and names both the file it checked
/// and the flag that carries the message. §FS-rhei-transition-cmd.3.2
#[test]
fn transition_into_a_terminal_state_without_a_result_is_refused() {
    let (dir, plan_path, machine_path) = setup_terminal_result_case("terminal-result-refused");

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "completed");
    assert!(
        !result.status.success(),
        "a terminal entry with no result must be refused\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(
        &result,
        "Task plan.1 cannot enter terminal state 'completed' without a result.",
    );
    assert_stderr_contains(&result, "runtime/results/plan.1.md");
    assert_stderr_contains(&result, "--result");

    // Refused before the state write: nothing moved and nothing was created.
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "a refused transition must not create the result file"
    );
    assert!(
        !dir.join("runtime/state-transitions.log").exists(),
        "a refused transition must not write a ledger line"
    );
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    assert!(plan.contains("**Assignee:** worker-1"), "the claim survives a refusal");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A result already on disk is the other way the obligation is met, so a bare
/// `rhei transition` into a terminal state succeeds once the worker has written
/// one. §FS-rhei-states.3.3
#[test]
fn a_result_already_on_disk_satisfies_a_bare_transition() {
    let (dir, plan_path, machine_path) = setup_terminal_result_case("terminal-result-on-disk");
    let results = dir.join("runtime/results");
    fs::create_dir_all(&results).expect("create results dir");
    fs::write(results.join("plan.1.md"), "## Result\n\nWorker wrote this by hand.\n")
        .expect("seed result");

    assert_success(&run_transition(&plan_path, &machine_path, "1", "pending", "completed"));
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// An existence-only contract would let an empty file stand in for an answer,
/// exactly as it would for a state handoff. §FS-rhei-states.3.3
#[test]
fn a_whitespace_only_result_file_counts_as_no_result() {
    let (dir, plan_path, machine_path) = setup_terminal_result_case("terminal-result-blank-file");
    let results = dir.join("runtime/results");
    fs::create_dir_all(&results).expect("create results dir");
    fs::write(results.join("plan.1.md"), "\n   \n\t\n").expect("seed blank result");

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "completed");
    assert!(
        !result.status.success(),
        "a whitespace-only result is no result\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "without a result");
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A `nextState` redirect is re-checked against the effective target, so a
/// callback cannot route a ticket into a terminal state the caller never asked
/// for and skip the result with it. §FS-rhei-transition-cmd.3.2
#[test]
fn a_callback_redirect_cannot_smuggle_a_terminal_entry_past_the_check() {
    let machine = r#"name: redirect-terminal-result
version: 1
states:
  pending:
    initial: true
    description: Not started
  in-progress:
    description: Working
  rejected:
    final: true
    description: Rejected outright
transitions:
  - from: pending
    to: in-progress
    on_leave: 'cli:printf ''{"success": true, "nextState": "rejected"}'''
  - from: pending
    to: rejected
"#;
    let dir = unique_temp_dir("terminal-result-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // The requested target is non-terminal; only the redirect is terminal.
    let result = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "in-progress"],
    );
    assert!(
        !result.status.success(),
        "the redirect must be held to the same rule\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "cannot enter terminal state 'rejected' without a result");
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    // With a message, the same redirect goes through and records it.
    assert_success(&run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "in-progress", "--result", RESULT_MESSAGE],
    ));
    assert_task_state(&plan_path, &machine_path, "1", "rejected");
    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains(RESULT_MESSAGE),
        "the redirect records the message; got:\n{recorded}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// An agent that exits `0` on an edge that finishes the ticket, without writing
/// a result, fails the completion condition — reported and routed exactly like
/// any other missing required output, naming the path that was checked.
// §FS-rhei-agents.3.2 §FS-rhei-run.3
#[test]
fn run_treats_a_missing_result_as_a_missing_required_output() {
    let (dir, plan_path, machine_path) = setup_terminal_result_case("terminal-result-run-silent");
    fs::write(&plan_path, ONE_TASK_PLAN.replace("**Assignee:** worker-1\n", ""))
        .expect("drop assignee");
    write_mock_agent_settings(&dir, &write_silent_agent(&dir));

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(
        !result.status.success(),
        "a run that cannot finish the only task exits non-zero\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("required outputs are missing"),
        "the missing result takes the missing-output route; got:\n{combined}"
    );
    assert!(
        combined.contains("result (runtime/results/plan.1.md)"),
        "the report must name the result path that was checked; got:\n{combined}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The engine ended this work, so the engine says why: the exit code lands in
/// the result file rather than an empty one. §FS-rhei-run.3
#[test]
fn a_run_failure_route_into_a_terminal_state_records_why() {
    let machine = r#"name: failing-program
version: 1
states:
  build:
    initial: true
    description: Build it
    program: "exit 3"
    program_timeout: 10s
  failed:
    final: true
    description: Gave up
transitions:
  - from: build
    to: failed
    exit_code: 3
"#;
    let plan = r#"# Rhei: Failing Program

## Tasks

### Task 1: Build
**State:** build
"#;
    let dir = unique_temp_dir("terminal-result-failure-route");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "failed");

    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains("exited 3") && recorded.contains("build"),
        "the failure route records the exit code and the state; got:\n{recorded}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Callback-only advancement has no subprocess that could know better, so the
/// engine records that it took the edge itself and that no worker answered —
/// the fact the old, empty result file withheld. §FS-rhei-run.3
#[test]
fn callback_only_advancement_records_that_no_worker_ran() {
    let (dir, plan_path, machine_path) =
        setup_single_file("terminal-result-callback-only", INDEPENDENT_PLAN);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains("no worker result was recorded"),
        "callback-only advancement says so in the result; got:\n{recorded}"
    );
    assert!(
        recorded.contains("'pending'"),
        "the engine names the state it advanced from; got:\n{recorded}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Taking `--result` and ignoring an empty value would write exactly the blank
/// result the obligation refuses. §FS-rhei-transition-cmd.2 §FS-rhei-complete.4
#[test]
fn a_blank_result_message_is_rejected_on_both_verbs() {
    let (dir, plan_path, machine_path) = setup_terminal_result_case("terminal-result-blank-flag");

    let completed =
        run_cli("complete", &plan_path, &machine_path, &["--task", "1", "--result", "   "]);
    assert!(!completed.status.success(), "a blank --result is not a result");
    assert_stderr_contains(&completed, "--result carries no message");

    let transitioned = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "completed", "--result", ""],
    );
    assert!(!transitioned.status.success(), "a blank --result is not a result");
    assert_stderr_contains(&transitioned, "--result carries no message");

    assert_task_state(&plan_path, &machine_path, "1", "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}
