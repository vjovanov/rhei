// Supervision from the surfaces a person uses: what `rhei next` claims,
// refuses, and renders; what `rhei list --ready` calls work; and what
// `rhei reset` and `rhei validate` say about a supervisor. Its own part
// because these drive one command and read its output, while the file next
// door drives whole runs and reads the prompts they produced.

// §FS-rhei-supervision

use std::fs;

use super::supervision_barrier_tests::TWO_CHILD_PLAN;
use super::supervision_tests::{setup_supervision, supervision_machine};
use super::*;

/// §FS-rhei-supervision.3.4: `rhei next` never claims a descendant of a held
/// supervisor, and names the supervisor rather than reporting a stall.
#[test]
fn rhei_next_reports_a_held_descendant_instead_of_claiming_it() {
    let plan = TWO_CHILD_PLAN
        .replace("**State:** supervise\n", "**State:** supervise\n**Assignee:** pi\n");
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-next-held",
        &plan,
        &supervision_machine("task", "completed"),
        "",
    );

    let targeted = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert!(!targeted.status.success(), "a held descendant is not claimable");
    assert_stderr_contains(&targeted, "held by supervisor Task plan.1 (supervise)");

    let auto = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!auto.status.success(), "nothing else is claimable either");
    assert_stderr_contains(&auto, "ticket(s) held by a supervisor");
    assert_stderr_contains(&auto, "Task plan.1.1 held by supervisor Task plan.1 (supervise)");
    // §FS-rhei-supervision.3.4: the visit is already claimed here, so the row
    // names who holds it and how to hand it back.
    assert_stderr_contains(&auto, "pi holds it");
    assert_stderr_contains(&auto, "rhei release");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A worker whose whole scope is held is told which ticket to work.
///
/// The supervisor is in no other category the diagnosis reports — its own
/// subtree is open, so the workable set excludes it — and a row that stopped
/// at "everything is held" left the worker with nowhere to go.
// §FS-rhei-supervision.3.4
#[test]
fn the_held_row_names_the_supervisor_as_the_ticket_to_work() {
    // `supervise` is not the profile's initial state, so nothing is
    // auto-claimable and the diagnosis is all the worker gets.
    let machine = r#"name: midflow
version: 1
states:
  plan: { initial: true, description: Plan, agent: mock, agent_timeout: 30s, instructions: plan }
  supervise:
    description: Supervise
    supervise: task
    agent: mock
    agent_timeout: 30s
    visits: 12
    instructions: supervise
  fix: { description: Fix, agent: mock, agent_timeout: 30s, instructions: fix }
  completed: { description: Done, final: true }
  cancelled: { description: Dropped, final: true }
transitions:
  - { from: plan, to: supervise, description: Start supervising }
  - { from: supervise, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervise, to: supervise, description: Released }
  - { from: fix, to: completed, description: Fixed }
  - { from: "*", to: cancelled, description: Dropped }
"#;
    let plan = r#"# Rhei: Mid

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
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-held-next-step", plan, machine, "");

    let diagnosed = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!diagnosed.status.success(), "nothing is auto-claimable");
    assert_stderr_contains(&diagnosed, "Task plan.1.1 held by supervisor Task plan.1");
    assert_stderr_contains(&diagnosed, "Work the supervisor instead");
    assert_stderr_contains(&diagnosed, "--task plan.1");

    // And the command it names is one that works.
    let worked = run_cli("next", &plan_path, &machine_path, &["--task", "plan.1", "--peek"]);
    assert_success(&worked);

    fs::remove_dir_all(dir).expect("cleanup");
}

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

/// `rhei next` renders the same two supervision sections `rhei run` composes:
/// the checkpoints a supervisor is owed, and the brief its descendant was
/// written.
// §FS-rhei-supervision.3.4
#[test]
fn rhei_next_renders_the_checkpoints_and_the_brief_the_run_prompt_would() {
    let plan = r#"# Rhei: Handover

---
structure:
  maxLevels: 3
metadata:
  tasks:
    1:
      stateVisits:
        supervise: 2
      supervision:
        phase: released
---

## Tasks

### Task 1: Parent
**State:** supervise-2

#### Task 1.1: A
**State:** fix
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-next-sections",
        plan,
        &supervision_machine("task", "completed"),
        "",
    );
    let brief = dir.join("runtime/supervise");
    fs::create_dir_all(&brief).expect("create brief dir");
    fs::write(brief.join("plan.1.1.md"), "Fix only the parser overflow.\n").expect("write brief");

    // §FS-rhei-supervision.5.2: the released descendant reads its brief.
    let child = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert_success(&child);
    assert!(child.stdout.contains("## Supervisor Brief"), "got:\n{}", child.stdout);
    assert!(child.stdout.contains("Fix only the parser overflow."), "got:\n{}", child.stdout);

    // §FS-rhei-supervision.5.1: and the supervisor reads its checkpoints, in
    // JSON as a field of its own.
    let done =
        run_cli("complete", &plan_path, &machine_path, &["--task", "1.1", "--result", "fixed"]);
    assert_success(&done);
    let supervisor =
        run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek", "--json"]);
    assert_success(&supervisor);
    let payload: serde_json::Value =
        serde_json::from_str(&supervisor.stdout).expect("next --json parses");
    assert!(
        payload["checkpoints"]
            .as_str()
            .expect("a checkpoint section")
            .contains("### Task plan.1.1: A \u{2014} fix \u{2192} completed"),
        "got: {payload}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A plan with no supervising state prints exactly what it printed before
/// supervision existed: no empty section, no blank line, no field.
// §FS-rhei-supervision.3.4
#[test]
fn rhei_next_output_is_unchanged_for_a_plan_without_supervision() {
    let machine = r#"name: plain
version: 1
states:
  fix:
    initial: true
    description: Fix
    agent: mock
    agent_timeout: 30s
    instructions: Apply the fixes.
  completed:
    description: Done
    final: true
transitions:
  - { from: fix, to: completed, description: Fixed }
"#;
    let plan = r#"# Rhei: Plain

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Fix findings
**State:** fix

Body text.

#### Task 1.1: A
**State:** fix
"#;
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-next-untouched", plan, machine, "");

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek"]);
    assert_success(&peek);
    let expected = [
        "Task plan.1.1 \u{2014} current state: 'fix' (read-only peek; not advanced)",
        "Agent: mock  |  Model: default",
        "",
        "## Task plan.1.1: A",
        "",
        "--- Instructions (fix) ---",
        "Apply the fixes.",
        "",
    ]
    .join("\n");
    assert_eq!(peek.stdout, expected);

    let json = run_cli("next", &plan_path, &machine_path, &["--task", "1.1", "--peek", "--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("next --json parses");
    assert!(payload.get("checkpoints").is_none(), "got: {payload}");
    assert!(payload.get("supervisor_brief").is_none(), "got: {payload}");

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
