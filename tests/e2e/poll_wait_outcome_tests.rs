//! A poll state's handled wait, read back from the two durable records a run
//! leaves behind. The ticket this pins reported one scenario — a two-attempt
//! poll whose first attempt exits 75 into the declared self-loop — and two
//! complaints about it: the attempt was journalled `outcome=failed`, and the
//! self-loop it took appears nowhere in the state ledger. Both answers are
//! here, and so is the seam between them: the attempt that *ends* the waiting
//! keeps the outcome its exit earns.

// §FS-rhei-states.2.2 §FS-rhei-run-tui.1.7 §FS-rhei-complete.3.1

use std::fs;
use std::path::Path;

use super::*;

/// The ticket's own plan: one task, parked in the poll state.
const RULING_PLAN: &str = r#"# Rhei: Poll waiting attempt

## Tasks

### Task 1: Wait for the ruling
**State:** awaiting-ruling
"#;

/// The ticket's own machine shape: two attempts, an `exit_code` self-loop, and
/// a condition-only exhaustion edge, so the edge that ends the wait is not the
/// one the exit code matched.
fn ruling_machine(command: &str, self_loop: &str) -> String {
    format!(
        r#"name: poll-waiting-ledger
version: 1
states:
  awaiting-ruling:
    description: Poll until the external ruling is available
    program:
      command: {command}
    poll:
      interval: 0s
      max_attempts: 2
  completed:
    description: Done
    final: true
transitions:
  - from: awaiting-ruling
    to: awaiting-ruling
    {self_loop}
  - from: awaiting-ruling
    to: completed
    condition: pollAttempts >= pollMaxAttempts
"#
    )
}

fn run_run(plan_path: &Path, machine_path: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(isolated_home_for(plan_path));
    if let Some(parent) = plan_path.parent() {
        cmd.current_dir(parent);
    }
    cmd.arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("run command should execute");
    CliRun::from(&output)
}

/// The two records this ticket is about, read off disk after the run: the run
/// event journal's released-attempt lines, and the whole state ledger.
struct PollTrail {
    ends: Vec<String>,
    ledger: String,
}

fn read_poll_trail(dir: &Path) -> PollTrail {
    let journal =
        fs::read_to_string(dir.join("runtime/transitions.log")).expect("read run event journal");
    let ends = journal
        .lines()
        .filter(|line| line.contains("end@awaiting-ruling"))
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2, "the fixture runs exactly two attempts; journal was:\n{journal}");
    PollTrail {
        ends,
        ledger: fs::read_to_string(dir.join("runtime/state-transitions.log"))
            .expect("read state transition ledger"),
    }
}

/// Stand up the fixture and run it to completion, returning what it wrote.
fn poll_trail(prefix: &str, body: &str, self_loop: &str, extra_args: &[&str]) -> PollTrail {
    let dir = unique_temp_dir(prefix);
    let script = write_python_agent(&dir, "poll.py", body);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", RULING_PLAN);
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &ruling_machine(&fixture_command(&script), self_loop),
    );

    let mut args = vec!["--no-tui", "--no-callbacks"];
    args.extend_from_slice(extra_args);
    let result = run_run(&plan_path, &machine_path, &args);
    assert_success(&result);

    let trail = read_poll_trail(&dir);
    // The fixture is only evidence while it still ends the way the ticket's
    // did: the exhaustion edge taken, the task finished.
    assert!(
        trail.ledger.contains("awaiting-ruling@completed"),
        "the fixture should have reached its terminal state; ledger was:\n{}",
        trail.ledger
    );
    trail
}

/// A program that exits 75 on every attempt, the way a checker says "the ruling
/// is not in yet".
const EXIT_75: &str = "sys.exit(75)\n";

/// A program that exits 0 on every attempt and writes the ticket's result, so
/// the exhaustion edge into a `final: true` state has the artifact it owes.
/// §FS-rhei-states.3.3
const EXIT_0_WITH_RESULT: &str = "result('the ruling landed\\n')\nsys.exit(0)\n";

fn outcome_of(line: &str) -> &str {
    line.rsplit_once("outcome=")
        .map(|(_, outcome)| outcome)
        .unwrap_or_else(|| panic!("a released attempt records its outcome; line was:\n{line}"))
}

/// The ticket, pinned, and the seam beside it. Three readings of one run:
///
/// 1. the attempt whose exit matched the declared self-loop was a wait and is
///    journalled as one;
/// 2. the attempt that spent the budget is not — its exit matched no edge and
///    the exhaustion condition routed it, so it keeps `failed`. That half is
///    `agent-grounds/rhei#200`, and asserting it here is what keeps a fix for
///    this ticket from quietly taking it too;
/// 3. the ledger still holds the one move that happened. A self-loop is handled
///    inside the engine, so it appends nothing — today's behaviour, asserted so
///    it is a decision rather than an accident that can drift.
// §FS-rhei-states.2.2 §FS-rhei-run-tui.1.7 §FS-rhei-complete.3.1
#[test]
fn a_poll_self_loop_attempt_is_journalled_as_a_wait() {
    let trail = poll_trail("poll-wait-exit75", EXIT_75, "exit_code: 75", &["--parallel", "1"]);

    assert_eq!(
        outcome_of(&trail.ends[0]),
        "waiting",
        "the attempt that took the self-loop is a handled wait, not a failure:\n{}",
        trail.ends[0]
    );
    assert_eq!(
        outcome_of(&trail.ends[1]),
        "failed",
        "the attempt that spent the budget keeps the outcome its exit earns:\n{}",
        trail.ends[1]
    );
    assert_eq!(
        trail.ledger, "plan.1 awaiting-ruling@completed\n",
        "the ledger records moves, and a poll self-loop is not one"
    );
}

/// The widening the ticket's own example never shows. The commonest poll exits
/// `0` and takes a condition-only self-loop; that attempt reads `completed`
/// today, which gives one situation two words. A wait is a wait whichever exit
/// matched the edge.
// §FS-rhei-states.2.2
#[test]
fn an_exit_zero_poll_self_loop_is_a_wait_and_not_a_completion() {
    let trail = poll_trail(
        "poll-wait-exit0",
        EXIT_0_WITH_RESULT,
        "condition: pollAttempts < pollMaxAttempts",
        &["--parallel", "1"],
    );

    assert_eq!(
        outcome_of(&trail.ends[0]),
        "waiting",
        "an exit-0 attempt that took the self-loop has not finished the state:\n{}",
        trail.ends[0]
    );
    assert_eq!(
        outcome_of(&trail.ends[1]),
        "completed",
        "the attempt that left the state on exit 0 really did complete:\n{}",
        trail.ends[1]
    );
}

/// The same answer from the other execution path. `--parallel 2` runs programs
/// through the worker pool, where the slot is released by a different piece of
/// code than in sequential mode, so the two paths are asked the same question
/// rather than one standing in for both.
// §FS-rhei-run.5 §FS-rhei-run-tui.1.7
#[test]
fn the_worker_pool_journals_a_poll_wait_like_the_sequential_path() {
    let trail = poll_trail("poll-wait-parallel", EXIT_75, "exit_code: 75", &["--parallel", "2"]);

    assert_eq!(
        outcome_of(&trail.ends[0]),
        "waiting",
        "the pooled path must agree with the sequential one:\n{}",
        trail.ends[0]
    );
    assert_eq!(
        outcome_of(&trail.ends[1]),
        "failed",
        "and must keep the exhaustion attempt's own outcome:\n{}",
        trail.ends[1]
    );
    assert_eq!(
        trail.ledger, "plan.1 awaiting-ruling@completed\n",
        "the pooled path appends no self-loop edge either"
    );
}

/// The same shape, backed by an `agent` rather than a `program`. The two
/// release their slots through different code — an agent's goes out from the
/// completion handler that decides what its exit meant, a program's from the
/// loop that reaped it — so the rule of §FS-rhei-states.2.2 has to reach both.
const RULING_AGENT_MACHINE: &str = r#"name: poll-waiting-agent
version: 1
states:
  awaiting-ruling:
    description: Poll until the external ruling is available
    agent: mock
    agent_timeout: 30s
    poll:
      interval: 0s
      max_attempts: 2
    instructions: Say whether the ruling has landed.
  completed:
    description: Done
    final: true
transitions:
  - from: awaiting-ruling
    to: awaiting-ruling
    condition: pollAttempts < pollMaxAttempts
  - from: awaiting-ruling
    to: completed
    condition: pollAttempts >= pollMaxAttempts
"#;

/// An agent that finishes its turn and writes the ticket's result, so the
/// exhaustion edge into a `final: true` state has the artifact it owes.
/// §FS-rhei-states.3.3
const AGENT_WITH_RESULT: &str = "result('the ruling landed\\n')\nsys.exit(0)\n";

/// Stand up the agent-backed fixture, with the settings that resolve `mock` to
/// the script, and run it to completion.
fn agent_poll_trail(prefix: &str, extra_args: &[&str]) -> PollTrail {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", RULING_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", RULING_AGENT_MACHINE);
    let script = write_python_agent(&dir, "mock-agent.py", AGENT_WITH_RESULT);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{ "mock": {{ "command": {command}, "prompt_flag": "--prompt", "timeout": "30s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let mut args = vec!["--no-tui", "--no-callbacks"];
    args.extend_from_slice(extra_args);
    let result = run_run(&plan_path, &machine_path, &args);
    assert_success(&result);

    let trail = read_poll_trail(&dir);
    assert!(
        trail.ledger.contains("awaiting-ruling@completed"),
        "the fixture should have reached its terminal state; ledger was:\n{}",
        trail.ledger
    );
    trail
}

/// The rule is about a poll state's attempt, not about what ran it. An agent
/// polling for a ruling waits exactly as a program does, and its attempt is
/// journalled the same way — including the exit-`0` case, where "completed"
/// would otherwise claim the state was finished by an agent that had only
/// asked to be woken again.
// §FS-rhei-states.2.2 §FS-rhei-run-tui.1.7
#[test]
fn an_agent_backed_poll_self_loop_attempt_is_journalled_as_a_wait() {
    let trail = agent_poll_trail("poll-wait-agent", &["--parallel", "1"]);

    assert_eq!(
        outcome_of(&trail.ends[0]),
        "waiting",
        "an agent that took the self-loop has not finished the state:\n{}",
        trail.ends[0]
    );
    assert_eq!(
        outcome_of(&trail.ends[1]),
        "completed",
        "the attempt that left the state on exit 0 really did complete:\n{}",
        trail.ends[1]
    );
    assert_eq!(
        trail.ledger, "plan.1 awaiting-ruling@completed\n",
        "an agent's self-loop is handled internally too, so it appends no line"
    );
}

/// And from the worker pool, where an agent's release crosses a channel to the
/// thread that selects its transition.
// §FS-rhei-run.5 §FS-rhei-states.2.2
#[test]
fn the_worker_pool_journals_an_agent_poll_wait_like_the_sequential_path() {
    let trail = agent_poll_trail("poll-wait-agent-parallel", &["--parallel", "2"]);

    assert_eq!(
        outcome_of(&trail.ends[0]),
        "waiting",
        "the pooled agent path must agree with the sequential one:\n{}",
        trail.ends[0]
    );
    assert_eq!(
        outcome_of(&trail.ends[1]),
        "completed",
        "and must keep the leaving attempt's own outcome:\n{}",
        trail.ends[1]
    );
}
