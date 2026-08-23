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

/// A mock agent that writes `body` verbatim to the path Rhei told it to use.
/// Passing the path in `RHEI_RESULT_PATH` is the contract a program has instead
/// of a prompt. §FS-rhei-agents.4
fn write_result_writing_agent(dir: &Path, body: &str) -> PathBuf {
    // The body is JSON-encoded, which is also a Python string literal, so a
    // quote or a backslash in it cannot escape into the script.
    let literal = serde_json::to_string(body).expect("result body json");
    write_python_agent(dir, "mock-agent.py", &format!("result({literal})\n"))
}

/// A worker that exits `code` having written nothing.
fn write_exiting_agent(dir: &Path, name: &str, code: i32) -> PathBuf {
    write_python_agent(dir, name, &format!("sys.exit({code})\n"))
}

fn write_silent_agent(dir: &Path) -> PathBuf {
    write_exiting_agent(dir, "mock-agent.py", 0)
}

fn write_mock_agent_settings(workspace_root: &Path, script: &Path) {
    let settings_dir = workspace_root.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{
    "mock": {{ "command": {command}, "timeout": "10s" }}
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
    assert_eq!(trail.ledger, "plan.1 pending@completed\n", "{driver}: ledger line differs");
    assert_eq!(trail.result, expected_result, "{driver}: result file differs");
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

fn setup_terminal_result_case(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", AGENT_TERMINAL_MACHINE);
    (dir, plan_path, machine_path)
}

/// The test of done: `rhei complete --result`, `rhei transition --result`, and
/// `rhei run` (whose agent wrote the result file) all take the same edge and
/// leave the same ledger line, the same result file, the same `> **Result:**`
/// link, and no `**Assignee:**`.
///
/// The comparison is byte-for-byte. Rhei appends a carried message as the
/// heading, a blank line, the message, and a trailing blank line, and takes a
/// worker-written file verbatim — so the two routes coincide exactly when the
/// worker writes that entry, which is what the mock agent here does. A worker
/// that writes something else keeps its own bytes, by design; that latitude is
/// what makes the equality worth pinning rather than assuming.
// §FS-rhei-complete.4 §FS-rhei-complete.3.2 §FS-rhei-states.3.3
#[test]
fn every_verb_leaves_the_same_terminal_trail() {
    // The exact bytes `append_result_entry` writes for one carried message.
    let expected = format!("## Result\n\n{RESULT_MESSAGE}\n\n");

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
    assert_eq!(by_complete.result, by_run.result, "byte-identical, not merely equivalent");

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
    // Absolute, because in a Panta project the result lives under the owning
    // rhei's root and a relative path is one the operator cannot paste.
    // §FS-rhei-agents.3.2.1
    let expected = std::path::absolute(dir.join("runtime/results/plan.1.md"))
        .unwrap_or_else(|_| dir.join("runtime/results/plan.1.md"));
    assert!(
        combined.contains(&format!("result ({})", expected.display())),
        "the report must name the result path that was checked; got:\n{combined}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "pending");

    // The durable report says the same thing. It used to say "stalled in
    // non-terminal state pending — inspect logs", naming neither the file nor
    // an action that would produce it. §FS-rhei-run-report.3.1
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("worker exited 0 without") && report.contains("result ("),
        "the report names the artifact the worker did not write; got:\n{report}"
    );
    assert!(
        !report.contains("inspect logs or mark the task cancelled"),
        "a named halt must not fall back to the generic advice; got:\n{report}"
    );
}

/// The engine ended this work, so the engine says why: the exit code lands in
/// the result file rather than an empty one. §FS-rhei-run.3
#[test]
fn a_run_failure_route_into_a_terminal_state_records_why() {
    let dir = unique_temp_dir("terminal-result-run-failure");
    let failing = write_exiting_agent(&dir, "failing-program.py", 3);
    let machine = format!(
        r#"name: failing-program
version: 1
states:
  build:
    initial: true
    description: Build it
    program:
      command: {command}
    program_timeout: 10s
  failed:
    final: true
    description: Gave up
transitions:
  - from: build
    to: failed
    exit_code: 3
"#,
        command = fixture_command(&failing)
    );
    let plan = r#"# Rhei: Failing Program

## Tasks

### Task 1: Build
**State:** build
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "failed");

    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert!(
        recorded.contains("exited 3") && recorded.contains("build"),
        "the failure route records the exit code and the state; got:\n{recorded}"
    );
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
}

/// Taking `--result` and ignoring an empty value would write exactly the blank
/// result the obligation refuses. §FS-rhei-transition-cmd.2 §FS-rhei-complete.4
#[test]
fn a_blank_result_message_is_rejected_on_both_verbs() {
    let (_dir, plan_path, machine_path) = setup_terminal_result_case("terminal-result-blank-flag");

    let completed =
        run_cli("complete", &plan_path, &machine_path, &["--task", "1", "--result", "   "]);
    assert!(!completed.status.success(), "a blank --result is not a result");
    assert_stderr_contains(&completed, "--result carries no message");

    // An argument check, so it runs before the plan loads: a caller who typed a
    // bad ticket *and* a blank message hears about the flag they got wrong.
    // §FS-rhei-complete.4
    let unknown_task =
        run_cli("complete", &plan_path, &machine_path, &["--task", "99", "--result", "  "]);
    assert!(!unknown_task.status.success());
    assert_stderr_contains(&unknown_task, "--result carries no message");

    let transitioned = run_cli(
        "transition",
        &plan_path,
        &machine_path,
        &["--task", "1", "--from", "pending", "--to", "completed", "--result", ""],
    );
    assert!(!transitioned.status.success(), "a blank --result is not a result");
    assert_stderr_contains(&transitioned, "--result carries no message");

    assert_task_state(&plan_path, &machine_path, "1", "pending");
}

/// `rhei complete` whose `on_leave` redirects to a **non-terminal** state: the
/// move happened, so the ledger has it and the caller's message rides with it,
/// and `complete` still exits non-zero because the ticket is not finished.
///
/// The recorded message then satisfies the obligation at the eventual terminal
/// edge, exactly as any earlier `transition --result` on the same ticket does.
// §FS-rhei-complete.4 §FS-rhei-states.3.3
#[test]
fn complete_redirected_to_a_non_terminal_state_still_records_the_message() {
    let machine = r#"name: redirect-non-terminal
version: 1
states:
  pending:
    initial: true
    description: Not started
  review:
    description: Sent back for review
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
    on_leave: 'cli:printf ''{"success": true, "nextState": "review"}'''
  - from: pending
    to: review
  - from: review
    to: completed
"#;
    let dir = unique_temp_dir("terminal-result-complete-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", RESULT_MESSAGE],
    );
    assert!(
        !result.status.success(),
        "the caller asked to finish a ticket the machine sent elsewhere\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // The move is the machine's decision and it stands, message included.
    assert_task_state(&plan_path, &machine_path, "1", "review");
    let history =
        fs::read_to_string(dir.join("runtime/state-transitions.log")).expect("read ledger");
    assert_eq!(history, "plan.1 pending@review\n");
    let recorded =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read result file");
    assert_eq!(recorded, format!("## Result\n\n{RESULT_MESSAGE}\n\n"));

    // And it pre-satisfies the obligation at the real terminal edge.
    assert_success(&run_transition(&plan_path, &machine_path, "1", "review", "completed"));
    assert_task_state(&plan_path, &machine_path, "1", "completed");
}

/// `rhei next`'s auto-advance out of a setup-only initial state never *declares*
/// an edge into a terminal state, but an `on_leave` redirect can still put one
/// there. The shared path refuses it and the plan is left untouched: `next`
/// claims work, it does not finish it.
// §FS-rhei-next.3 §FS-rhei-states.3.3
#[test]
fn next_auto_advance_redirected_into_a_terminal_state_is_refused_cleanly() {
    let machine = r#"name: next-redirect-terminal
version: 1
states:
  planning:
    initial: true
    description: Setup only
  pending:
    description: Ready for work
  completed:
    final: true
    description: Done
transitions:
  - from: planning
    to: pending
    on_leave: 'cli:printf ''{"success": true, "nextState": "completed"}'''
  - from: planning
    to: completed
  - from: pending
    to: completed
"#;
    let plan = r#"# Rhei: Next Redirect

## Tasks

### Task 1: Do the work
**State:** planning
"#;
    let dir = unique_temp_dir("terminal-result-next-redirect");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &[]);
    assert!(
        !result.status.success(),
        "a claim must not finish the ticket by redirect\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "cannot enter terminal state 'completed' without a result");
    assert_task_state(&plan_path, &machine_path, "1", "planning");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "a refused claim must not create the result file"
    );
}

/// A fanned-out state gives every invocation its own result fragment, and `run`
/// merges them into the ticket's result before the terminal transition. One
/// shared path would have made the last writer erase every sibling's account.
// §FS-rhei-states.3.3 §FS-rhei-run.3
const FANOUT_TERMINAL_MACHINE: &str = r#"name: fanout-terminal-result
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: Every reviewer weighs in
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: review
    to: completed
"#;

const FANOUT_PLAN: &str = r#"# Rhei: Fanout Result

## Tasks

### Task 1: Review from every angle
**State:** review
"#;

/// Settings whose `mock` agent runs `script`, with a model registry both fan-out
/// targets resolve through.
fn write_fanout_agent_settings(workspace_root: &Path, script: &Path) {
    let settings_dir = workspace_root.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{
    "mock": {{ "command": {command}, "timeout": "10s" }}
  }},
  "models": {{
    "alpha": {{ "provider": "mock", "model": "alpha", "default_agent": "mock" }},
    "beta": {{ "provider": "mock", "model": "beta", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
}

#[test]
fn a_fanned_out_terminal_edge_keeps_every_invocation_s_account() {
    let dir = unique_temp_dir("terminal-result-fanout");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TERMINAL_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"result('{} reviewed it.\n'.format(env('RHEI_MODEL')))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    // Each invocation wrote its own fragment, keyed by its identity …
    for identity in ["mock-mock-alpha", "mock-mock-beta"] {
        assert!(
            dir.join(format!("runtime/results/plan.1/review/1/{identity}.md")).exists(),
            "{identity}: fan-out invocation writes its own fragment, keyed by state and visit"
        );
    }

    // … and the merged result carries both, attributed, in declared order.
    let merged =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("merged result file");
    assert!(merged.contains("alpha reviewed it."), "model-a's account survives; got:\n{merged}");
    assert!(merged.contains("beta reviewed it."), "model-b's account survives; got:\n{merged}");
    assert!(
        merged.contains("## Result \u{2014} mock-mock-alpha")
            && merged.contains("## Result \u{2014} mock-mock-beta"),
        "each entry names the invocation it came from; got:\n{merged}"
    );
    assert!(
        merged.find("mock-mock-alpha") < merged.find("mock-mock-beta"),
        "entries follow declared invocation order; got:\n{merged}"
    );
}

/// The completion condition is per invocation, so a fan-out worker that writes
/// nothing fails its own — the sibling that did write is not an answer for it.
// §FS-rhei-agents.3.2 §FS-rhei-states.3.3
#[test]
fn a_fanned_out_invocation_that_writes_nothing_fails_its_own_completion_condition() {
    let dir = unique_temp_dir("terminal-result-fanout-silent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TERMINAL_MACHINE);
    // Only `alpha` answers.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"if env('RHEI_MODEL') == 'alpha':
    result('alpha reviewed it.\n')
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(
        !result.status.success(),
        "the silent invocation must fail its own condition\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("plan.1/review/1/mock-mock-beta.md"),
        "the warning names the fragment the silent invocation owed; got:\n{combined}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "review");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "nothing is merged when the state did not finish"
    );
}

/// The same fan-out state, but the terminal state demands an `inputs:` artifact
/// nothing writes, so the move is refused after the fragments are merged. The
/// merge must survive that and must not re-append itself on the next attempt.
// §FS-rhei-states.3.3
const FANOUT_REFUSED_MACHINE: &str = r#"name: fanout-refused
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: Every reviewer weighs in
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  completed:
    final: true
    description: Done
    inputs:
      - name: sign-off
        path: runtime/sign-off/{task_id}.md
        required: true
transitions:
  - from: review
    to: completed
"#;

/// A fan-out result is merged **once**, when the last fragment lands — not once
/// per invocation that exits, and not again on a retry over the same fragments.
/// Appending per invocation left four entries for a ticket that never moved.
// §FS-rhei-states.3.3
#[test]
fn a_refused_fan_out_move_merges_the_fragments_exactly_once_per_attempt() {
    let dir = unique_temp_dir("terminal-result-fanout-once");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_REFUSED_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"result('{} reviewed it.\n'.format(env('RHEI_MODEL')))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let first = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!first.status.success(), "the missing target input refuses the move");
    assert_task_state(&plan_path, &machine_path, "1", "review");

    let entries = |label: &str| {
        let merged = fs::read_to_string(dir.join("runtime/results/plan.1.md"))
            .unwrap_or_else(|err| panic!("{label}: merged result file: {err}"));
        assert!(
            merged.matches("## Result \u{2014} mock-mock-alpha").count() == 1
                && merged.matches("## Result \u{2014} mock-mock-beta").count() == 1,
            "{label}: one entry per invocation, no more; got:\n{merged}"
        );
    };
    entries("first pass");

    // A second run rewrites the same fragments, so the merged block is the one
    // already on disk and nothing is appended.
    let second = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!second.status.success(), "still refused");
    entries("second pass");
}

/// Two fanned-out states over the same targets. Keyed by identity alone, every
/// `refine` invocation would find `review`'s fragment already on disk, write
/// nothing, and hand the ticket `review`'s account as its result.
// §FS-rhei-states.3.3 §FS-rhei-agents.3.2
const FANOUT_TWO_STATE_MACHINE: &str = r#"name: fanout-two-states
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: First look
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  refine:
    description: Second look
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: review
    to: refine
  - from: refine
    to: completed
"#;

#[test]
fn a_second_fanned_out_state_does_not_inherit_the_first_s_fragments() {
    let dir = unique_temp_dir("terminal-result-fanout-stale");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TWO_STATE_MACHINE);
    // Writes only in `review`; `refine` exits 0 having written nothing.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"if env('RHEI_STATE') == 'review':
    result('STALE from review by {}.\n'.format(env('RHEI_MODEL')))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(
        !result.status.success(),
        "`refine` wrote nothing, so it fails its own completion condition\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "refine");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("plan.1/refine/1/mock-mock-alpha.md"),
        "the warning names the fragment `refine` owed, under its own state; got:\n{combined}"
    );
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "the ticket never finished, so it has no result — least of all `review`'s"
    );
    for identity in ["mock-mock-alpha", "mock-mock-beta"] {
        assert!(
            dir.join(format!("runtime/results/plan.1/review/{identity}.md")).exists()
                || dir.join(format!("runtime/results/plan.1/review/1/{identity}.md")).exists(),
            "{identity}: `review`'s fragment stays where `review` put it"
        );
    }
}

/// One invocation finishing is not the state finishing. With a sibling still
/// running, attempting the merge produced `1 of its fan-out invocation(s) wrote
/// no result` on a run where nothing was wrong.
// §FS-rhei-states.3.3
#[test]
fn a_slow_fan_out_sibling_does_not_raise_a_false_alarm() {
    let dir = unique_temp_dir("terminal-result-fanout-slow");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TERMINAL_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"model = env('RHEI_MODEL')
if model == 'beta':
    time.sleep(2)
result('{} reviewed it.\n'.format(model))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("wrote no result"),
        "a healthy run must not accuse the sibling that had not finished yet; got:\n{combined}"
    );
    let merged =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("merged result file");
    assert!(
        merged.contains("alpha reviewed it.") && merged.contains("beta reviewed it."),
        "both accounts survive; got:\n{merged}"
    );
}

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
            report.contains("plan.rhei.md --task plan.1 --from probe"),
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
