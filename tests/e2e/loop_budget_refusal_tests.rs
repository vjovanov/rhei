//! §FS-rhei-errors.1.5: a refused loop re-entry names the state whose budget
//! the check measured, and says how far that budget is spent.

use super::*;

/// Assert that stderr does *not* contain `forbidden`, collapsed the same way
/// [`assert_stderr_contains`] collapses, so miette's wrapping cannot hide a
/// phrase from the negative assertion either.
fn assert_stderr_lacks(result: &CliRun, forbidden: &str) {
    fn collapse(text: &str) -> String {
        text.chars().filter(|c| c.is_ascii_graphic()).collect()
    }
    assert!(
        !collapse(&result.stderr).contains(&collapse(forbidden)),
        "expected stderr not to contain {:?}; got:\n{}",
        forbidden,
        result.stderr
    );
}

/// The reported shape reduced to four states: `supervising` is the only state
/// with a budget, and `human-review` is a gate that declares none, so a refused
/// `human-review -> supervising` has exactly one state worth naming.
const CROSS_STATE_MACHINE: &str = r#"name: reentry-budget-cross-state
version: 1
states:
  pending:
    initial: true
    description: Ready
  supervising:
    description: The supervisor decides; the only state with a visit budget
    visits: 2
  human-review:
    description: Human gate; declares no visit budget at all
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: supervising
  - from: supervising
    to: human-review
  - from: human-review
    to: supervising
  - from: human-review
    to: cancelled
"#;

const SELF_LOOP_MACHINE: &str = r#"name: reentry-budget-self-loop
version: 1
states:
  pending:
    initial: true
    description: Ready
  fix:
    description: A counted self-loop
    visits: 2
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: fix
  - from: fix
    to: fix
  - from: fix
    to: cancelled
"#;

const POLL_MACHINE: &str = r#"name: reentry-budget-poll
version: 1
states:
  pending:
    initial: true
    description: Ready
  ci-wait:
    description: Waits on a remote CI run
    poll:
      interval: 1s
      max_attempts: 3
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: ci-wait
  - from: ci-wait
    to: ci-wait
  - from: ci-wait
    to: cancelled
"#;

const PLAN: &str = r#"# Rhei: Loop Re-entry Budget

## Tasks

### Task 1: Supervised work
**State:** pending
"#;

/// `rhei transition` spends the *destination* state's `visits` when it decides
/// a loop re-entry (§FS-rhei-transitions.4.3), so a refusal naming the state
/// being left points the reader at a state with no budget to raise.
// §FS-rhei-errors.1.5
#[test]
fn refused_cross_state_reentry_names_the_destination_whose_budget_is_spent() {
    let dir = unique_temp_dir("loop-budget-cross-state");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", CROSS_STATE_MACHINE);

    // Spend `supervising`'s two visits and park the task in `human-review`.
    // The engine writes `stateVisits` itself: nothing here is hand-authored.
    for (from, to) in [
        ("pending", "supervising"),
        ("supervising", "human-review"),
        ("human-review", "supervising"),
        ("supervising", "human-review"),
    ] {
        assert_success(&run_transition(&plan_path, &machine_path, "1", from, to));
    }

    let result = run_transition(&plan_path, &machine_path, "1", "human-review", "supervising");
    assert!(!result.status.success(), "a spent visit budget must still refuse the re-entry");
    assert_stderr_contains(
        &result,
        "visit budget for state 'supervising' is exhausted (2/2 visits)",
    );
    assert_stderr_lacks(&result, "visit budget for state 'human-review'");
    assert_task_state(&plan_path, &machine_path, "1", "human-review");
}

/// The self-loop case is correct today and must stay correct: a fix that reads
/// the destination at the format site gets this one right only because the two
/// states are the same, and one that keeps reading the current state gets the
/// cross-state case wrong. Both are pinned so neither can be traded for the
/// other.
// §FS-rhei-errors.1.5
#[test]
fn refused_self_loop_names_its_own_state_and_its_visit_usage() {
    let dir = unique_temp_dir("loop-budget-self-loop");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", SELF_LOOP_MACHINE);

    assert_success(&run_transition(&plan_path, &machine_path, "1", "pending", "fix"));
    assert_success(&run_transition(&plan_path, &machine_path, "1", "fix", "fix"));

    let result = run_transition(&plan_path, &machine_path, "1", "fix", "fix");
    assert!(!result.status.success(), "a spent visit budget must still refuse the self-loop");
    assert_stderr_contains(&result, "visit budget for state 'fix' is exhausted (2/2 visits)");
    assert_task_state(&plan_path, &machine_path, "1", "fix-2");
}

/// A poll state's budget is `poll.max_attempts`, which is mutually exclusive
/// with `visits` and raised by a different key, so a refusal that calls it a
/// visit budget sends the reader to a field the state cannot declare.
// §FS-rhei-errors.1.5 §FS-rhei-states.2.2
#[test]
fn refused_poll_self_loop_names_its_attempt_budget() {
    // `rhei transition` maintains `stateVisits` only for states that count
    // visits, so a poll state's attempt counter is authored here the way
    // `rhei run` would have left it after three attempts.
    let plan = r#"# Rhei: Loop Re-entry Budget

---
metadata:
  tasks:
    1:
      stateVisits:
        ci-wait: 3
---

## Tasks

### Task 1: Waiting on CI
**State:** ci-wait
"#;
    let dir = unique_temp_dir("loop-budget-poll");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", POLL_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "1", "ci-wait", "ci-wait");
    assert!(!result.status.success(), "a spent poll budget must still refuse the self-loop");
    assert_stderr_contains(&result, "poll budget for state 'ci-wait' is exhausted (3/3 attempts)");
    assert_stderr_lacks(&result, "visit budget");
}
