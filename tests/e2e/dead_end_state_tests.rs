//! A state nothing can leave is refused when the machine is loaded, instead of
//! validating clean and stranding the first task that reaches it.
//!
//! §FS-rhei-states.1.3 is the rule, §FS-rhei-transitions.4.6 is what decides
//! whether a wildcard edge counts as a way out, and §FS-rhei-states.8.2 asks
//! the same question of one profile's narrowed state set.

use std::path::PathBuf;

use super::*;

/// The plan from the report: one task, starting in the state with no way out.
const REPORTED_PLAN: &str = r#"# Rhei: Dead-end repro

## Tasks

### Task 1: Reach the dead-end state
**State:** specify
"#;

/// The machine from the report, with `specify_edge` spliced in where the
/// author's forgotten `from: specify` transition belongs.
///
/// `specify` runs a program that writes `specify-ran.txt` beside the plan, so a
/// test can tell whether a run ever spawned anything before it gave up.
fn reported_workspace(prefix: &str, specify_edge: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let program = write_python_agent(
        &dir,
        "specify.py",
        "write(pathlib.Path(env('RHEI_ROOT')) / 'specify-ran.txt', 'ran\\n')\n",
    );
    let machine = format!(
        r#"name: dead-end-repro
version: 1.0
states:
  specify:
    description: Write the spec. Nothing takes the task out of here.
    program:
      command: {command}
  implement:
    description: Build it.
    program:
      command: {command}
  completed:
    description: Done.
    final: true
  cancelled:
    description: Abandoned.
    final: true
transitions:
{specify_edge}  - from: implement
    to: completed
    exit_code: 0
  - from: "*"
    to: cancelled
    description: Cancel the task from any non-final state.
profiles:
  default:
    initial: specify
    allowed: [specify, implement, completed, cancelled]
node_policy:
  root: default
  default: default
"#,
        command = fixture_command(&program),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", REPORTED_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine);
    (dir, plan_path, machine_path)
}

/// The forgotten edge, as the valid half of the report's fixture pair spells it.
const RESTORED_EDGE: &str = "  - from: specify\n    to: implement\n    exit_code: 0\n";

/// Hold a refusal to the substance the report asks for: the state that cannot
/// be left, the wildcard target that is not counted as progress, and the line
/// to write. The wording is the implementer's; these three facts are not.
fn assert_names_the_dead_end(result: &CliRun, state: &str, wildcard_target: &str) {
    assert!(
        !result.status.success(),
        "a machine with a state nothing can leave must be refused; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(result, state);
    assert_stderr_contains(result, wildcard_target);
    assert_stderr_contains(result, &format!("from: {state}"));
}

/// The report itself: `specify` has no `from: specify` rule, so the only rule
/// matching it is the machine-wide `* -> cancelled`, which `rhei run` never
/// takes. §FS-rhei-states.1.3
#[test]
fn validate_refuses_a_state_left_only_by_a_wildcard_to_a_final_state() {
    let (_dir, plan_path, machine_path) = reported_workspace("dead-end-reported", "");

    let result = run_cli("validate", &plan_path, &machine_path, &[]);

    assert_names_the_dead_end(&result, "specify", "cancelled");
}

/// A guard, not a contract: the valid half of the report's fixture pair passes
/// today and must keep passing. §FS-rhei-transitions.4.6
#[test]
fn validate_accepts_the_reported_machine_once_the_missing_edge_is_restored() {
    let (_dir, plan_path, machine_path) = reported_workspace("dead-end-restored", RESTORED_EDGE);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);

    assert_success(&result);
    assert!(
        result.stdout.contains("Validation succeeded"),
        "expected a clean validation; got:\n{}",
        result.stdout
    );
}

/// The cost the report leads with: the refusal lands before a task is
/// scheduled, so no agent and no program is ever spawned. §FS-rhei-states.1.3
#[test]
fn run_refuses_the_dead_end_machine_before_spawning_anything() {
    let (dir, plan_path, machine_path) = reported_workspace("dead-end-run", "");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);

    // The report's cost, first: today the run spawns the program, strands the
    // task, and only then halts. The refusal has to land before that.
    assert!(
        !dir.join("specify-ran.txt").exists(),
        "the run spawned the program in 'specify'; the machine must be refused first.\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_names_the_dead_end(&result, "specify", "cancelled");
}

/// A deliberate consequence of putting the rule in the loader rather than in
/// plan validation: `rhei transition` loads the machine too, so the by-hand
/// escape hatch is not available for this rule. Cancelling the stranded task is
/// legal on this machine and succeeds today; once the machine is refused, the
/// repair is the YAML line the error prints, not a move by hand.
/// §FS-rhei-states.1.3
#[test]
fn transition_is_refused_on_the_dead_end_machine_too() {
    let (_dir, plan_path, machine_path) = reported_workspace("dead-end-transition", "");

    let result = run_transition_with_result(
        &plan_path,
        &machine_path,
        "1",
        "specify",
        "cancelled",
        "## Result\n\nStranded; abandoned by hand.\n",
    );

    assert_names_the_dead_end(&result, "specify", "cancelled");
}

/// Most machines in the wild declare no `profiles` block, so the per-profile
/// reachability check never runs on them. The rule is asked of the machine
/// itself, not only of its profiles. §FS-rhei-states.1.3
#[test]
fn validate_refuses_a_dead_end_state_in_a_machine_that_declares_no_profiles() {
    let dir = unique_temp_dir("dead-end-legacy");
    let machine = r#"name: dead-end-legacy
version: 1
states:
  draft:
    description: Write it
  review:
    description: Nothing declares an edge out of here
  completed:
    description: Done
    final: true
  cancelled:
    description: Abandoned
    final: true
transitions:
  - from: draft
    to: review
  - from: draft
    to: completed
  - from: "*"
    to: cancelled
"#;
    let plan = r#"# Rhei: Legacy Dead End

## Tasks

### Task 1: Write it
**State:** draft
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);

    assert_names_the_dead_end(&result, "review", "cancelled");
}

/// A guard: out of a `gating: true` state the wildcard cancel is an edge a
/// human really can take with `rhei transition`, so the state is not stranded
/// and the machine still loads — both before this change and after.
/// §FS-rhei-transitions.4.6
#[test]
fn validate_accepts_a_gating_state_left_only_by_a_wildcard_cancel() {
    let dir = unique_temp_dir("dead-end-gating");
    let machine = r#"name: gating-cancel-only
version: 1
states:
  draft:
    description: Write it
  blocked:
    description: Stop here for a human. Only cancellation leaves it.
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Abandoned
    final: true
transitions:
  - from: draft
    to: blocked
  - from: draft
    to: completed
  - from: "*"
    to: cancelled
"#;
    let plan = r#"# Rhei: Gating Cancel Only

## Tasks

### Task 1: Write it
**State:** draft
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);

    assert_success(&result);
}
