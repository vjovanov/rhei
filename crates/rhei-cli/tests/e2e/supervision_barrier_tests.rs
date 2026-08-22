// §FS-rhei-supervision, the rest of the barrier: what a supervisor may do to
// its subtree during a visit, how a nested one behaves, what `--parallel`
// drains, and what every surface says about a held ticket.
use std::fs;

use super::supervision_tests::{
    assert_state_anywhere, prompt_for, setup_supervision, spawn_log, supervision_machine,
};
use super::*;

const TWO_CHILD_PLAN: &str = r#"# Rhei: Harden

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Harden the parser
**State:** supervise

#### Task 1.1: Review parser
**State:** review

#### Task 1.2: Fix findings
**State:** fix
"#;

/// §FS-rhei-supervision.2.1 and §6: a supervisor cancels a step during its own
/// visit; that move is its own doing, so it is not a checkpoint, and the plan
/// it left behind is what `openDescendants` reads on the way out.
#[test]
fn a_step_the_supervisor_cancels_is_not_reported_back_to_it() {
    let cancel = r#"    if [ "$visit" = "1" ]; then
      "RHEI_BIN" --state-machine "$RHEI_STATE_MACHINE_PATH" transition "$RHEI_PLAN_PATH" \
        --task "$task.2" --from fix --to cancelled --result "made unnecessary" --no-callbacks
    fi"#
    .replace("RHEI_BIN", env!("CARGO_BIN_EXE_rhei"));
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-cancel",
        TWO_CHILD_PLAN,
        &supervision_machine("task", "completed"),
        &cancel,
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    assert_state_anywhere(&plan_path, &machine_path, "1.2", "cancelled");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervise 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervise 2".to_string(),
        ],
        "the cancelled step never runs, and the supervisor is woken once for the one that did"
    );

    let second = prompt_for(&dir, "plan.1", "supervise", 2);
    assert!(second.contains("### Task plan.1.1:"), "got:\n{second}");
    assert!(
        !second.contains("### Task plan.1.2:"),
        "the supervisor's own cancel is not news for it; got:\n{second}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-supervision.2.2: a checkpoint reaches exactly one task — the
/// nearest supervising ancestor — and the inner supervisor's own terminal exit
/// is what the outer one hears.
#[test]
fn a_nested_supervisor_hears_its_own_subtree_and_reports_upward_when_it_finishes() {
    let plan = r#"# Rhei: Nested

---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Top
**State:** supervise

#### Task 1.1: Middle
**State:** supervise

##### Task 1.1.1: Leaf
**State:** review
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-nested",
        plan,
        &supervision_machine("task", "completed"),
        "",
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1.1", "completed");
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervise 1".to_string(),
            "plan.1.1 supervise 1".to_string(),
            "plan.1.1.1 review 1".to_string(),
            "plan.1.1 supervise 2".to_string(),
            "plan.1 supervise 2".to_string(),
        ],
        "the outer supervisor releases the inner one, which releases the leaf"
    );

    let inner = prompt_for(&dir, "plan.1.1", "supervise", 2);
    assert!(
        inner.contains("### Task plan.1.1.1:"),
        "the leaf checkpoints its nearest; got:\n{inner}"
    );
    let outer = prompt_for(&dir, "plan.1", "supervise", 2);
    assert!(
        outer.contains("### Task plan.1.1: Middle \u{2014} supervise \u{2192} completed"),
        "the outer supervisor hears only the inner one's own exit; got:\n{outer}"
    );
    assert!(
        !outer.contains("### Task plan.1.1.1:"),
        "an ancestor farther up sees nothing of the leaf; got:\n{outer}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-supervision.3.1: under `--parallel` a checkpoint is a drain —
/// siblings already running finish, nothing new starts, and the supervisor sees
/// every checkpoint they produced in one visit.
#[test]
fn a_checkpoint_drains_the_parallel_siblings_before_the_supervisor_runs() {
    let machine = supervision_machine("task", "completed")
        .replace("  review:\n", "  review:\n    concurrent: true\n")
        .replace("  fix:\n", "  fix:\n    concurrent: true\n");
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-parallel", TWO_CHILD_PLAN, &machine, "");

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-callbacks", "--no-tui", "--parallel", "2"],
    );
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "completed");
    let log = spawn_log(&dir);
    assert_eq!(log.first().map(String::as_str), Some("plan.1 supervise 1"));
    assert_eq!(log.last().map(String::as_str), Some("plan.1 supervise 2"));
    assert_eq!(log.len(), 4, "both children run inside the one release; got:\n{log:?}");

    // §FS-rhei-supervision.3.3: the checkpoints accumulate and one visit
    // consumes them all.
    let second = prompt_for(&dir, "plan.1", "supervise", 2);
    assert!(second.contains("### Task plan.1.1:"), "got:\n{second}");
    assert!(second.contains("### Task plan.1.2:"), "got:\n{second}");

    fs::remove_dir_all(dir).expect("cleanup");
}

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

/// A checkpoint names one descendant exactly, and spells it the way
/// `## Child Tasks` and `rhei transition` do.
///
/// The ids here collide on the tail — the sibling `1.2` beside the nested
/// `1.1.2` — which is the case a suffix match resolves to the wrong task and
/// then shows the supervisor that task's title and result.
// §FS-rhei-supervision.5.1
#[test]
fn a_checkpoint_names_the_descendant_that_moved_not_one_whose_id_ends_the_same() {
    let plan = r#"# Rhei: Nested

---
structure:
  maxLevels: 4
---

## Tasks

### Task 1: Outer
**State:** supervise

#### Task 1.1: Inner supervisor
**State:** supervise

##### Task 1.1.1: A
**State:** fix

##### Task 1.1.2: B
**State:** fix
**Prior:** Task 1.1.1

#### Task 1.2: Sibling
**State:** fix
**Prior:** Task 1.1
"#;
    let (dir, plan_path, machine_path) = setup_supervision(
        "supervision-tail-collision",
        plan,
        &supervision_machine("task", "completed"),
        "",
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    let outer = prompt_for(&dir, "plan.1", "supervise", 3);
    assert!(
        outer.contains("### Task plan.1.2: Sibling \u{2014} fix \u{2192} completed (visit 1)"),
        "the checkpoint is the sibling, titled and qualified as such; got:\n{outer}"
    );
    assert!(
        outer.contains("Task plan.1.2 finished fix."),
        "and it carries the sibling's own result; got:\n{outer}"
    );
    assert!(
        !outer.contains("Task plan.1.1.2 finished fix."),
        "the tail-colliding cousin's result is not the sibling's; got:\n{outer}"
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
