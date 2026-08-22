// Supervision from the surfaces that are not `rhei next`: what `rhei validate`
// rejects and warns about, what `rhei reset` clears, what `rhei list --ready`
// calls work, and what the run report says about a supervisor that cannot
// finish. What `rhei next` says lives next door in `supervision_next_tests.rs`.

// §AR-source-file-size.3 §FS-rhei-supervision

use std::fs;

use super::supervision_barrier_tests::TWO_CHILD_PLAN;
use super::supervision_tests::{setup_supervision, supervision_machine, REVIEW_FIX_PLAN};
use super::*;

/// §FS-rhei-supervision.3.3: `rhei reset` clears the supervision block beside
/// `stateVisits`.
#[test]
fn rhei_reset_clears_the_supervision_block() {
    let plan = r#"# Rhei: Reset

---
structure:
  maxLevels: 3
metadata:
  tasks:
    1:
      stateVisits:
        supervise: 3
      supervision:
        phase: held
        checkpoints:
          - task: "1.1"
            from: review
            to: completed
            visit: 1
---

## Tasks

### Task 1: Harden the parser
**State:** supervise

#### Task 1.1: Review parser
**State:** review
"#;
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-reset", plan, &supervision_machine("task", "completed"), "");

    let result = run_cli("reset", &plan_path, &machine_path, &["-y"]);
    assert_success(&result);
    let after = fs::read_to_string(&plan_path).expect("read plan");
    assert!(!after.contains("supervision:"), "got:\n{after}");
    assert!(!after.contains("stateVisits"), "got:\n{after}");
    // §FS-rhei-reset: the entry those two filled goes with them — a reset plan
    // carries no `tasks: {1: {}}` for the next reader to interpret.
    assert!(!after.contains("tasks:"), "got:\n{after}");
    assert!(!after.contains("metadata:"), "got:\n{after}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-supervision.1.2: the machine is rejected before anything runs when
/// a supervisor could never be scheduled, and warned about when it could never
/// finish.
#[test]
fn supervise_validation_rejects_and_warns_through_rhei_validate() {
    // The workspace registers the `mock` agent the machines name, so the only
    // findings left are the supervision rules.
    let (dir, plan_path, _machine_path) = setup_supervision(
        "supervision-validate",
        TWO_CHILD_PLAN,
        &supervision_machine("task", "completed"),
        "",
    );

    let no_self_loop = supervision_machine("task", "completed")
        .replace("  - { from: supervise, to: supervise, description: Released the subtree }\n", "");
    let bad = write_fixture_file(&dir, "no-self-loop.yaml", &no_self_loop);
    let result = run_cli("validate", &plan_path, &bad, &[]);
    assert!(!result.status.success(), "a supervisor with no release edge is rejected");
    assert_stderr_contains(&result, "no self-loop transition");

    let bad_value =
        supervision_machine("task", "completed").replace("supervise: task", "supervise: subtree");
    let bad = write_fixture_file(&dir, "bad-value.yaml", &bad_value);
    let result = run_cli("validate", &plan_path, &bad, &[]);
    assert!(!result.status.success(), "an unknown supervise value is rejected");
    assert_stderr_contains(&result, "expected 'task' or 'state'");

    let no_exit = supervision_machine("task", "completed").replace(
        "  - { from: supervise, to: completed, description: Subtree done, condition: openDescendants < 1 }\n",
        "",
    );
    let warned = write_fixture_file(&dir, "no-exit.yaml", &no_exit);
    let result = run_cli("validate", &plan_path, &warned, &[]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("no way to finish"),
        "a supervisor with no terminal edge is warned about; got:\n{combined}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
/// The manual-worker loop of §3.4 runs to the end: claim, release, let a child
/// finish, and claim the visit that child's checkpoint earned.
///
/// The release self-loop ends the visit, so it ends the claim. A claim that
/// survived it swallowed every later checkpoint and left the subtree with
/// nothing anyone could work.
// §FS-rhei-supervision.3.4
#[test]
fn the_release_self_loop_hands_the_supervisor_back_to_the_next_worker() {
    let plan = r#"# Rhei: Manual

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** supervise

#### Task 1.1: A
**State:** fix

#### Task 1.2: B
**State:** fix
**Prior:** Task 1.1
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-manual-release",
        plan,
        &supervision_machine("task", "completed"),
        "",
    );

    let claim = run_cli("next", &plan_path, &machine_path, &["--task", "1"]);
    assert_success(&claim);
    assert!(
        fs::read_to_string(&plan_path).expect("read plan").contains("**Assignee:**"),
        "the visit is claimed"
    );

    let released = run_transition(&plan_path, &machine_path, "1", "supervise", "supervise");
    assert_success(&released);
    assert!(
        !fs::read_to_string(&plan_path).expect("read plan").contains("**Assignee:**"),
        "the self-loop ends the visit, and with it the claim"
    );

    // §FS-rhei-supervision.2.1: with no claim standing, the child's own exit is
    // news for the supervisor rather than the supervisor's own doing.
    let done =
        run_cli("complete", &plan_path, &machine_path, &["--task", "1.1", "--result", "fixed"]);
    assert_success(&done);
    let plan_after = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        plan_after.contains("phase: held"),
        "the checkpoint holds the subtree again; got:\n{plan_after}"
    );
    assert!(plan_after.contains("task: '1.1'"), "and it is recorded; got:\n{plan_after}");

    // §FS-rhei-supervision.3.4: the command the held descendant's help names is
    // the command that works.
    let held = run_cli("next", &plan_path, &machine_path, &["--task", "1.2", "--peek"]);
    assert!(!held.status.success(), "a held descendant is still not claimable");
    assert_stderr_contains(&held, "--task plan.1");

    let second = run_cli("next", &plan_path, &machine_path, &["--task", "1"]);
    assert_success(&second);
    assert!(
        second.stdout.contains("claimed"),
        "the next visit is claimable; got:\n{}",
        second.stdout
    );
    // §FS-rhei-supervision.3.4: the visit `rhei next` hands over carries what
    // the visit is about.
    assert!(
        second.stdout.contains("### Task plan.1.1: A \u{2014} fix \u{2192} completed (visit 1)"),
        "got:\n{}",
        second.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// `rhei list --ready` returns exactly what `rhei run` would schedule and
/// `rhei next` would claim, at every phase of the barrier.
///
/// Three surfaces answering "what is work" from two different rules is how a
/// listing tells an operator to work a ticket the run refuses to schedule.
// §FS-rhei-supervision.3.2
#[test]
fn list_ready_and_the_ready_set_agree_at_every_phase() {
    let held = r#"# Rhei: Ready

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** supervise

#### Task 1.1: A
**State:** fix
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-list-ready",
        held,
        &supervision_machine("task", "completed"),
        "",
    );

    // Held: the supervisor is the work, its descendant is not.
    let ready = run_cli("list", &plan_path, &machine_path, &["--ready"]);
    assert_success(&ready);
    assert!(ready.stdout.contains("Task plan.1:"), "got:\n{}", ready.stdout);
    assert!(!ready.stdout.contains("Task plan.1.1:"), "got:\n{}", ready.stdout);

    // Released: the descendant is the work, the supervisor is not.
    assert_success(&run_transition(&plan_path, &machine_path, "1", "supervise", "supervise"));
    let ready = run_cli("list", &plan_path, &machine_path, &["--ready"]);
    assert_success(&ready);
    assert!(!ready.stdout.contains("Task plan.1:"), "got:\n{}", ready.stdout);
    assert!(ready.stdout.contains("Task plan.1.1:"), "got:\n{}", ready.stdout);

    // Released with the subtree already closed: nobody is work, and the
    // listing says so rather than offering a ticket the run refuses.
    assert_success(&run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1.1", "--result", "fixed"],
    ));
    assert_success(&run_transition(&plan_path, &machine_path, "1", "supervise", "supervise"));
    let ready = run_cli("list", &plan_path, &machine_path, &["--ready"]);
    assert_success(&ready);
    assert!(!ready.stdout.contains("Task plan.1:"), "got:\n{}", ready.stdout);
    let next = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!next.status.success(), "and `rhei next` refuses it too");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A machine whose supervisor cannot finish is legal, so `rhei run` runs it —
/// and says, twice, what is wrong with it.
///
/// Before: the warning existed only in `rhei validate`, and the run drove the
/// whole subtree and then reported "stalled in non-terminal state supervise /
/// inspect logs", which is advice for a halt nobody can name. This one is
/// nameable: the machine is missing one line.
// §FS-rhei-supervision.1.2 §FS-rhei-supervision.4.1 §FS-rhei-run.3
#[test]
fn a_supervisor_with_no_open_descendants_edge_is_told_which_line_is_missing() {
    let machine = supervision_machine("task", "completed")
        .replace(
            "  - { from: supervise, to: completed, description: Subtree done, condition: openDescendants < 1 }\n",
            "",
        );
    assert!(
        !machine.contains("openDescendants"),
        "the fixture must be the machine that cannot finish:\n{machine}"
    );
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-no-terminal-edge", REVIEW_FIX_PLAN, &machine, "");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    // The machine's own warning, printed at run start rather than only by
    // `rhei validate`.
    assert!(
        result.stderr.contains(
            "warning: state 'supervise' declares 'supervise' but no transition from it \
             reaches a final state on `openDescendants`"
        ),
        "got stderr:\n{}",
        result.stderr
    );
    // And the halt names the line to add, wherever the halt is reported.
    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("no transition out of 'supervise' is eligible on `openDescendants`"),
        "got:\n{report}"
    );
    assert!(
        report.contains("add `- {from: supervise, to: completed, condition: openDescendants < 1}`"),
        "got:\n{report}"
    );
    assert!(
        !report.contains("stalled in non-terminal state"),
        "the halt is nameable, so it must not fall back to the generic reading:\n{report}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// `rhei release` on a supervisor says nothing about moving it back.
///
/// The note exists because `rhei next` claims from the profile's initial state,
/// so a ticket released later is unclaimed but not re-claimable. A supervisor
/// is claimed exactly where it stands, so moving it back to `pending` is the
/// one thing that would make it unclaimable — and a machine that has no state
/// by that name cannot run the command the note prints at all.
// §FS-rhei-supervision.3.4 §FS-rhei-release
#[test]
fn releasing_a_supervisor_suggests_no_move_back() {
    let machine = r#"name: release-note
version: 1
states:
  pending:
    initial: true
    description: Fresh
  supervise:
    description: Supervise
    supervise: task
    agent: mock
    agent_timeout: 30s
    visits: 12
  review:
    description: Review
    agent: mock
    agent_timeout: 30s
  completed:
    description: Done
    final: true
transitions:
  - { from: pending, to: supervise, description: Start supervising }
  - { from: supervise, to: completed, description: Done, condition: openDescendants < 1 }
  - { from: supervise, to: supervise, description: Released }
  - { from: review, to: completed, description: Reviewed }
"#;
    let plan = r#"# Rhei: Release

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** supervise
**Assignee:** pi

#### Task 1.1: A
**State:** review
**Assignee:** pi
"#;
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-release-note", plan, machine, "");

    let supervisor = run_cli("release", &plan_path, &machine_path, &["--task", "1", "--dry-run"]);
    assert_success(&supervisor);
    assert!(
        !supervisor.stdout.contains("note: still in"),
        "a supervisor is claimed where it stands:\n{}",
        supervisor.stdout
    );

    // An ordinary ticket in a later state still gets the note.
    let child = run_cli("release", &plan_path, &machine_path, &["--task", "1.1", "--dry-run"]);
    assert_success(&child);
    assert!(
        child.stdout.contains("note: still in 'review'"),
        "the note is right for an ordinary ticket:\n{}",
        child.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// `rhei states` says when a supervisor wakes and what the granularity costs.
///
/// "Supervises: task" read as "supervises a task" — the one thing `supervise:`
/// does not mean. `--json` keeps the value itself, for scripts.
// §FS-rhei-supervision.1.1 §FS-rhei-states-cmd.4
#[test]
fn rhei_states_says_when_a_supervisor_wakes() {
    let plan = r#"# Rhei: States

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Parent
**State:** supervise

#### Task 1.1: A
**State:** review
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-states-cmd",
        plan,
        &supervision_machine("task", "completed"),
        "",
    );

    let by_task = run_cli("states", &plan_path, &machine_path, &[]);
    assert_success(&by_task);
    assert!(
        by_task.stdout.contains("Supervision: after every finished descendant (task)"),
        "got:\n{}",
        by_task.stdout
    );

    let state_machine =
        write_fixture_file(&dir, "state-level.yaml", &supervision_machine("state", "completed"));
    let by_state = run_cli("states", &plan_path, &state_machine, &[]);
    assert_success(&by_state);
    assert!(
        by_state.stdout.contains(
            "Supervision: after every descendant transition (state) \u{2014} one invocation \
             per hop"
        ),
        "got:\n{}",
        by_state.stdout
    );

    let json = run_cli("states", &plan_path, &state_machine, &["--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("states --json parses");
    let supervising = payload["states"]
        .as_array()
        .expect("states array")
        .iter()
        .find(|state| state["name"] == "supervise")
        .expect("the supervising state");
    assert_eq!(supervising["supervise"], "state", "scripts keep the value: {payload}");

    fs::remove_dir_all(dir).expect("cleanup");
}
