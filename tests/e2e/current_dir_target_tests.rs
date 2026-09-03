// Naming a Directory Workspace by the directory the command runs in. `.`, `./`,
// a bare `index.rhei.md`, and `..` from a subdirectory all name one rhei, and
// must read exactly as its absolute path does — the spelling an author reaches
// for from inside the workspace was the one spelling that failed (#120).

// §AR-source-file-size.3 §FS-rhei-panta.6

use std::path::Path;

use super::new_tests::{assert_failure, flattened_output};
use super::*;

const ONE_TICKET: &str = "### Task 1: First\n**State:** draft\n";

/// Run one rhei subcommand *from inside* `cwd`. The whole point of these cases
/// is the invocation directory, so the target is passed as a raw string rather
/// than as a path the harness has already resolved.
fn run_from(cwd: &Path, home: &Path, machine: &Path, subcommand: &str, target: &str) -> CliRun {
    let mut cmd = rhei_command(home);
    cmd.current_dir(cwd);
    cmd.arg("--state-machine").arg(machine).arg(subcommand).arg(target);
    let output = cmd.output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Every spelling of the workspace that only exists relative to a cwd lists the
/// same tickets, under the same ids, as its absolute path. §FS-rhei-panta.6
#[test]
fn current_directory_spellings_list_the_same_tickets() {
    let (dir, ws, machine) =
        create_workspace("cwd-target-list", "# Rhei: Current Directory\n", &[("a.md", ONE_TICKET)]);
    let home = dir.join(".home");

    let absolute = run_from(&ws, &home, &machine, "list", &ws.display().to_string());
    assert_success(&absolute);
    assert!(
        absolute.stdout.contains("Task workspace.1: First"),
        "the id is prefixed by the workspace directory name; got:\n{}",
        absolute.stdout
    );

    for spelling in [".", "./", "index.rhei.md", "./index.rhei.md", "../workspace"] {
        let result = run_from(&ws, &home, &machine, "list", spelling);
        assert_success(&result);
        assert_eq!(
            result.stdout, absolute.stdout,
            "`rhei list {spelling}` should read as the absolute path does"
        );
    }

    // `..` carries no name of its own either, and from `tasks/` it names the
    // workspace. §FS-rhei-panta.6
    let from_tasks = run_from(&ws.join("tasks"), &home, &machine, "list", "..");
    assert_success(&from_tasks);
    assert_eq!(from_tasks.stdout, absolute.stdout, "`rhei list ..` should name the workspace");
}

/// The same resolution serves every command, so `rhei validate .` succeeds on a
/// workspace that validates by its absolute path. §FS-rhei-panta.6
#[test]
fn validate_accepts_the_current_directory() {
    let (dir, ws, machine) = create_workspace(
        "cwd-target-validate",
        "# Rhei: Current Directory\n",
        &[("a.md", ONE_TICKET)],
    );
    let home = dir.join(".home");

    for spelling in [".", "./", "index.rhei.md"] {
        let result = run_from(&ws, &home, &machine, "validate", spelling);
        assert_success(&result);
        assert!(
            result.stdout.contains("Validation succeeded"),
            "`rhei validate {spelling}` should succeed; got:\n{}\n{}",
            result.stdout,
            result.stderr
        );
    }
}

/// A path that names no usable id is the path's problem, and the help says so
/// instead of sending the reader into a plan that parsed. §FS-rhei-panta.6
#[test]
fn a_path_that_names_no_id_is_reported_against_the_path() {
    let (dir, ws, machine) =
        create_workspace("cwd-target-bad-id", "# Rhei: Bad Id\n", &[("a.md", ONE_TICKET)]);
    let home = dir.join(".home");
    let renamed = dir.join("not.an.id");
    std::fs::rename(&ws, &renamed).expect("the workspace should be renamable");

    let result = run_from(&dir, &home, &machine, "list", &renamed.display().to_string());
    assert_failure(&result, "rhei id");
    let said = flattened_output(&result);
    assert!(
        said.contains("rhei id 'not.an.id'") && said.contains("is not valid"),
        "the message should name the id the path gave; got:\n{said}"
    );
    assert!(
        said.contains("a rhei's id comes from the path naming it"),
        "the help should point at the path; got:\n{said}"
    );
}

/// And the plan errors the same load reports keep the authoring help and the
/// code frame that go with them — a duplicate ticket id is not a path problem.
/// §FS-rhei-panta.6
#[test]
fn a_plan_error_keeps_its_authoring_help() {
    let dir = unique_temp_dir("cwd-target-duplicate");
    let plan = write_fixture_file(
        &dir,
        "duplicates.rhei.md",
        "# Rhei: Duplicates\n\n## Tasks\n\n### Task 1: First\n**State:** draft\n\n\
         ### Task 1: Again\n**State:** draft\n",
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let home = dir.join(".home");

    let result = run_from(&dir, &home, &machine, "list", &plan.display().to_string());
    assert_failure(&result, "duplicate task ID '1'");
    let said = flattened_output(&result);
    assert!(
        said.contains("check the plan's task metadata"),
        "a plan error keeps the authoring help; got:\n{said}"
    );
    assert!(
        !said.contains("a rhei's id comes from the path naming it"),
        "a plan error is not a path problem; got:\n{said}"
    );
}
