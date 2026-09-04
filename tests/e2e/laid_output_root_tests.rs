// A Panta project whose member rhei sits one directory below the project
// root, driven by `rhei run` end to end: the declared `outputs:` the agent
// writes under the execution root named in its prompt must use the same root the
// completion condition checks, both right after the agent exits and before a
// later pass decides whether to spawn it again.
//
// Its own part because every other e2e coverage of this machine runs a
// single-file or single-rhei plan, where the run-level workspace root and the
// owning rhei's execution root are the same directory and this bug is
// invisible.

// §FS-rhei-agents.3.2 §FS-rhei-panta.6.3

use std::fs;

use super::*;

const LAID_OUTPUT_MACHINE: &str = r#"name: laid-output-machine
version: 1
states:
  implement:
    initial: true
    description: Write the required report and finish the task
    agent: mock
    agent_timeout: 10s
    outputs:
      - name: report
        path: runtime/exports/{task_id}/report.md
  done:
    final: true
    description: Finished
transitions:
  - from: implement
    to: done
"#;

/// Writes the report the state declares and the ticket's terminal result,
/// both under the prompt's root — the member rhei's own execution root, not the
/// enclosing project's. Exactly what the agent environment promises it.
// §FS-rhei-agents.4 §FS-rhei-panta.6.3
const WRITE_REPORT_UNDER_PROMPT_ROOT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
write(str(root / 'runtime' / 'exports' / task / 'report.md'), '# Report\n')
result('## Result\n\nDone.\n')
"#;

/// A Panta project of one member rhei, `laid`, one directory below the
/// project root, with a single task sitting in `implement`. Returns
/// `(dir, project_root, machine_path)`.
fn setup_laid_output_project(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("project");
    fs::create_dir_all(project.join("laid").join("tasks")).expect("create member dirs");
    fs::write(project.join("laid").join("index.rhei.md"), "# Rhei: Laid Workflow\n")
        .expect("write member index");
    fs::write(
        project.join("laid").join("tasks").join("implement.md"),
        "### Task 1: Implement the laid workflow\n**State:** implement\n",
    )
    .expect("write member task");
    fs::write(project.join("index.panta.md"), "# Panta: Laid Output Root\n")
        .expect("write panta manifest");
    let machine_path = write_fixture_file(&dir, "states.yaml", LAID_OUTPUT_MACHINE);
    write_laid_output_agent_settings(&project);
    (dir, project, machine_path)
}

/// The mock agent under test, wired through settings so `rhei run` resolves
/// it for the `implement` state's `agent: mock`.
fn write_laid_output_agent_settings(project: &Path) {
    let script = write_python_agent(project, "mock-agent.py", WRITE_REPORT_UNDER_PROMPT_ROOT);
    let settings_dir = project.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{ "mock": {{ "command": {}, "stdin_prompt": true, "timeout": "10s" }} }}
}}"#,
            fixture_command(&script)
        ),
    )
    .expect("write settings");
}

/// vjovanov/rhei#137: the run scheduler's completion condition resolved
/// declared `outputs:` against the run-level workspace root instead of the
/// owning rhei's execution root — the same root the agent prompt and
/// the transition-time check already used. A member rhei one directory below
/// the project root wrote its report exactly where it was told and still
/// stalled: the agent's own output could never satisfy the state it belonged
/// to.
///
/// First run: the agent exits 0 having written both required artifacts under
/// its own root, and that alone must be enough to finish the ticket in the
/// same run — no second pass, no operator copying files up a directory.
/// Second run: with the ticket already terminal, nothing is left to spawn;
/// pre-fix, the unmet completion condition kept the ticket in `implement` and
/// a second run re-spawned the agent (the ticket's own transcript: `attempt 2
/// of 2`) even though its work was already on disk.
// §FS-rhei-agents.3.2 §FS-rhei-panta.6.3
#[test]
fn a_laid_member_rheis_outputs_satisfy_its_own_state_in_one_run() {
    let (_dir, project, machine_path) = setup_laid_output_project("laid-output-root");

    let first = run_cli("run", &project, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&first);
    assert!(
        first.stdout.contains("Spawning agent"),
        "the first run must spawn the agent; got:\n{}",
        first.stdout
    );
    assert!(
        !first.stdout.contains("required outputs are missing"),
        "the report the agent wrote under its own root must satisfy the state; got:\n{}",
        first.stdout
    );
    assert_task_state(&project, &machine_path, "laid.1", "done");
    assert!(
        project.join("laid/runtime/exports/laid.1/report.md").exists(),
        "the report belongs at the member root"
    );
    assert!(
        !project.join("runtime/exports/laid.1/report.md").exists(),
        "and nowhere else — this is not a workaround the agent should need"
    );

    let second = run_cli("run", &project, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&second);
    assert!(
        !second.stdout.contains("Spawning agent"),
        "a ticket already in a terminal state, its outputs already on disk, must not respawn; got:\n{}",
        second.stdout
    );
    assert_task_state(&project, &machine_path, "laid.1", "done");
}
