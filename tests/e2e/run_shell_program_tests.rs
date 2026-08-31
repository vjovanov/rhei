// A string-form `program:` is a command *line*, and a command line goes to the
// platform's own shell — `/bin/sh -c` on Unix, `cmd /S /C` on Windows.
//
// Its own file because it is the only end-to-end coverage of that spawn. Every
// other program state in the suite passes an argument vector, which never
// reaches a shell at all, and the string form is exactly where Windows used to
// die looking for a program named `sh`. Both routes out of the state are here,
// so the shell is asserted to hand back an exit status and not only to start.

// §FS-rhei-programs.1.1 §FS-rhei-programs.3 §REQ-cross-platform.2

use super::*;

const SHELL_PROGRAM_PLAN: &str = r#"# Rhei: Shell Program Run

## Tasks

### Task 1: Build artifact
**State:** build
"#;

/// A machine whose one working state is a *string* program, routed on the exit
/// code the shell reports back. §FS-rhei-programs.3
fn shell_program_machine(script: &Path) -> String {
    // `serde_json` rather than a hand-written scalar: the line carries an
    // absolute path, and on Windows that path is full of backslashes a plain
    // YAML double-quoted scalar would read as escapes.
    let command = serde_json::to_string(&fixture_command_line(script))
        .expect("program command line should serialize");
    format!(
        r#"name: shell-program-demo
version: 1
states:
  build:
    initial: true
    description: Build the artifact through the platform's own shell
    program: {command}
  completed:
    description: Done
    final: true
  failed:
    description: The build refused
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
  - from: build
    to: failed
    exit_code: nonzero
"#
    )
}

/// The shell starts the program, and a zero exit takes the success edge.
#[test]
fn a_string_form_program_runs_under_the_platform_shell() {
    let dir = unique_temp_dir("run-shell-program-ok");
    let program = write_python_agent(
        &dir,
        "build.py",
        r#"write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'shell-program.txt', 'ok\n')
result('## Result\n\nThe shell built the artifact.\n')
"#,
    );
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", SHELL_PROGRAM_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", &shell_program_machine(&program));

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(
        dir.join("runtime/shell-program.txt").exists(),
        "the shell-form program should have produced its artifact; got:\n{}",
        result.stdout
    );
}

/// And a non-zero exit comes back through the shell intact, so the `nonzero`
/// edge is the one that fires. §FS-rhei-programs.3
#[test]
fn a_string_form_program_routes_on_the_exit_code_the_shell_reports() {
    let dir = unique_temp_dir("run-shell-program-fail");
    let program = write_python_agent(
        &dir,
        "build.py",
        r#"write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'shell-program.txt', 'ran\n')
result('## Result\n\nThe shell refused the build.\n')
sys.exit(3)
"#,
    );
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", SHELL_PROGRAM_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", &shell_program_machine(&program));

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    // A routed exit code is a handled outcome, not a broken run: the run itself
    // succeeds and the ticket lands on the edge the code chose.
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "failed");
    assert!(
        result.stdout.contains("'build' -> 'failed'"),
        "the run should name the edge the exit code took; got:\n{}",
        result.stdout
    );
    assert!(
        dir.join("runtime/shell-program.txt").exists(),
        "the shell-form program should have run before it failed; got:\n{}",
        result.stdout
    );
}
