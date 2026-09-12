//! A program's exit that fires an exact `exit_code:` edge is the program naming
//! the route, not a subprocess failure the engine has to narrate. These pin the
//! split: an exact match leaves the ticket's result alone, a `"nonzero"`
//! catch-all still records why, and the classification follows the edge that
//! actually fired rather than the edges the state happens to declare.

// §FS-rhei-programs.3.2 §FS-rhei-run.3 §FS-rhei-states.3.3

use std::fs;
use std::path::Path;

use super::*;

const ROUTE_PLAN: &str = r#"# Rhei: Routing exit

## Tasks

### Task 1: Route on a meaningful exit
**State:** route
"#;

/// The ticket's own machine shape: one program state that exits 3, and whatever
/// edges out of it the case under test declares. `checked` is `gating: true` so
/// the run stops there with the ticket in a non-terminal state, which is the
/// half of the contract a `[ ! -s "$RHEI_RESULT_PATH" ]` guard depends on.
fn route_machine(command: &str, edges: &str) -> String {
    format!(
        r#"name: routing-exit
version: 1
states:
  route:
    initial: true
    description: Route on the exit code
    program:
      command: {command}
    program_timeout: 10s
  checked:
    description: Routed here by the declared edge
    gating: true
  build-failed:
    description: The program died
    gating: true
  completed:
    description: Done
    final: true
transitions:
{edges}
  - from: checked
    to: completed
  - from: build-failed
    to: completed
"#
    )
}

/// The same machine with its program state marked `concurrent: true`, which is
/// what lets one pass hold both tickets in `route` at once. Without the flag the
/// scheduler defers all but one ticket per pass, and the pool is entered one
/// program at a time — which asserts nothing the sequential loop does not.
fn concurrent_route_machine(command: &str, edges: &str) -> String {
    let sequential = route_machine(command, edges);
    let concurrent =
        sequential.replace("    initial: true\n", "    initial: true\n    concurrent: true\n");
    assert_ne!(concurrent, sequential, "the route state should have gained `concurrent: true`");
    concurrent
}

/// The ticket's result file as the run left it. Absent and empty are the same
/// answer — the engine wrote nothing — and which one it is depends on whether
/// some other rule happened to touch the file.
fn recorded_result(dir: &Path, task_id: &str) -> String {
    fs::read_to_string(dir.join(format!("runtime/results/{task_id}.md"))).unwrap_or_default()
}

fn set_up(
    prefix: &str,
    exit_code: i32,
    edges: &str,
) -> (TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = unique_temp_dir(prefix);
    let program = write_python_agent(&dir, "route.py", &format!("sys.exit({exit_code})\n"));
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ROUTE_PLAN);
    let machine_path =
        write_fixture_file(&dir, "states.yaml", &route_machine(&fixture_command(&program), edges));
    (dir, plan_path, machine_path)
}

/// The ticket verbatim: an `exit_code: 3` edge into a state that is not
/// terminal. The engine takes the edge and writes nothing, so a program that
/// wrote nothing leaves the result file empty and the next program in the
/// machine can be the one that fills it in.
// §FS-rhei-programs.3.2 §FS-rhei-run.3
#[test]
fn an_exact_route_into_a_non_terminal_state_records_nothing() {
    let (dir, plan_path, machine_path) = set_up(
        "declared-route-non-terminal",
        3,
        "  - from: route\n    to: checked\n    exit_code: 3\n",
    );

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "checked");

    let recorded = recorded_result(&dir, "plan.1");
    assert!(
        recorded.trim().is_empty(),
        "a declared route leaves the result file to the program; got:\n{recorded}"
    );
}

/// The other side of the split, and the reason the fix cannot simply delete the
/// message. A catch-all matched a code the program did not choose, so the
/// engine ended the work and still owes the account of it.
///
/// This passes before the change as well as after: it is the guard on the
/// removal, not a statement of the defect.
// §FS-rhei-programs.3.2 §FS-rhei-states.3.3
#[test]
fn a_nonzero_catch_all_still_records_why() {
    let (dir, plan_path, machine_path) = set_up(
        "declared-route-nonzero",
        3,
        "  - from: route\n    to: build-failed\n    exit_code: nonzero\n",
    );

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "build-failed");

    let recorded = recorded_result(&dir, "plan.1");
    assert!(
        recorded.contains("exited 3") && recorded.contains("route"),
        "a catch-all match is a failure the engine narrates; got:\n{recorded}"
    );
}

/// One state declaring both edges. The exact edge already wins selection, and
/// the result entry has to follow the edge that fired rather than the edges
/// available — otherwise the machine that most needs the distinction, the one
/// that routes some codes and fails on the rest, is the one that never gets it.
// §FS-rhei-programs.3.2
#[test]
fn an_exact_edge_beats_the_catch_all_and_records_nothing() {
    let (dir, plan_path, machine_path) = set_up(
        "declared-route-exact-wins",
        3,
        "  - from: route\n    to: build-failed\n    exit_code: nonzero\n  - from: route\n    to: checked\n    exit_code: 3\n",
    );

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "checked");

    let recorded = recorded_result(&dir, "plan.1");
    assert!(
        recorded.trim().is_empty(),
        "the entry follows the edge that fired, which wrote none; got:\n{recorded}"
    );
}

/// The case a fix that re-derived the classification from the machine would get
/// wrong. The exact edge matches the code but its `condition:` is false, so it
/// never fired; the catch-all did, and that is a genuine failure whose account
/// must survive. Asking the machine "is there an exact rule from here to there?"
/// after the fact cannot tell these apart, because it does not evaluate the
/// condition the selection evaluated.
///
/// This passes before the change as well as after.
// §FS-rhei-programs.3.2 §FS-rhei-transitions.4.3
#[test]
fn an_exact_edge_its_condition_disqualified_is_not_a_declared_route() {
    let (dir, plan_path, machine_path) = set_up(
        "declared-route-condition-false",
        3,
        "  - from: route\n    to: checked\n    exit_code: 3\n    condition: visitCount >= 2\n  - from: route\n    to: build-failed\n    exit_code: nonzero\n",
    );

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "build-failed");

    let recorded = recorded_result(&dir, "plan.1");
    assert!(
        recorded.contains("exited 3"),
        "an edge that did not fire cannot make the exit a declared route; got:\n{recorded}"
    );
}

/// The two program-completion paths are separate code, so the contract is
/// asserted on both: the worker pool must agree with `--parallel 1` about what
/// a declared route leaves behind.
///
/// Reaching the pool is a property of the fixture rather than of `--parallel`:
/// tickets that share one plan file are forced back to sequential execution, so
/// each ticket here has a file of its own in a directory workspace and `route` is
/// `concurrent: true`. The run's own `(parallel)` markers are asserted because a
/// fixture that drifts back to the sequential path would otherwise keep passing
/// while testing the path this test exists to cover twice over.
// §FS-rhei-run.3 §FS-rhei-run.5
#[test]
fn the_worker_pool_agrees_that_a_declared_route_records_nothing() {
    let tasks = [
        ("01-route.md", "### Task 1: Route on a meaningful exit\n**State:** route\n"),
        ("02-route.md", "### Task 2: Route on a meaningful exit as well\n**State:** route\n"),
    ];
    let (dir, workspace, machine_path) =
        create_workspace("declared-route-parallel", "# Rhei: Routing exit\n", &tasks);
    let program = write_python_agent(&dir, "route.py", "sys.exit(3)\n");
    fs::write(
        &machine_path,
        concurrent_route_machine(
            &fixture_command(&program),
            "  - from: route\n    to: checked\n    exit_code: 3\n",
        ),
    )
    .expect("state machine should be written");

    let result = run_cli(
        "run",
        &workspace,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--parallel", "2"],
    );
    assert_success(&result);

    let spawns = result
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("Spawning program for Task "))
        .collect::<Vec<_>>();
    assert_eq!(spawns.len(), 2, "one spawn per ticket; got:\n{}", result.stdout);
    assert!(
        !result.stdout.contains("Deferred"),
        "both tickets belong in one pass, or the pool holds one program at a time; got:\n{}",
        result.stdout
    );
    for spawn in &spawns {
        assert!(
            spawn.ends_with("(parallel)"),
            "the pool is what this test is for, so a run that fell back to the \
             sequential loop fails it; got:\n{spawn}"
        );
    }

    for task in ["1", "2"] {
        let suffix = format!(".{task}");
        let task_id = spawns
            .iter()
            .filter_map(|spawn| spawn.split_once(": "))
            .map(|(id, _)| id)
            .find(|id| id.ends_with(&suffix))
            .unwrap_or_else(|| panic!("Task {task} should have spawned; got {spawns:?}"));

        assert_task_state(&workspace, &machine_path, task, "checked");
        let recorded = recorded_result(&workspace, task_id);
        assert!(
            recorded.trim().is_empty(),
            "task {task}: the worker pool writes no entry either; got:\n{recorded}"
        );
    }
}

/// The other half of the contract, and where it becomes visible in a durable
/// artifact: a declared route into a `final: true` state leaves the result to
/// the program, so one that writes nothing stalls — and the run report names
/// the code that program actually exited with. A row reading `worker exited 0`
/// would contradict the same report's own ledger, which is the ticket's
/// complaint in another artifact.
// §FS-rhei-run-report.3.1 §FS-rhei-states.3.3
#[test]
fn a_routed_exit_that_owes_the_result_is_reported_with_its_own_exit_code() {
    let dir = unique_temp_dir("declared-route-stall-report");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ROUTE_PLAN);
    let program = write_python_agent(&dir, "route.py", "sys.exit(3)\n");
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &route_machine(
            &fixture_command(&program),
            "  - from: route\n    to: completed\n    exit_code: 3\n",
        ),
    );

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!result.status.success(), "the ticket owes a result it never wrote, so the run halts");
    assert_task_state(&plan_path, &machine_path, "1", "route");

    let report = fs::read_to_string(dir.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("worker exited 3 without result ("),
        "the halt names the exit the worker really had; got:\n{report}"
    );
    assert!(
        !report.contains("worker exited 0 without"),
        "and never an exit that did not happen; got:\n{report}"
    );
}
