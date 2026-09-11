// An agent that advances its own ticket while it works, and what the run makes
// of that visit. The plan re-read after the exit supplies the operands the
// edge selection evaluates and nothing else: the edge still leaves the state
// the invocation ran in, so a worker that moved its own ticket onward is not
// judged against the edge out of the state it moved it to.

// §FS-rhei-agents.3.2

use std::path::{Path, PathBuf};

use super::snapshot_tests::{run_snapshot_command, write_fake_snapshot_settings};
use super::*;

/// `implement` emits a snapshot `on: success`, which is how the run writes down
/// what it made of the visit: the snapshot exists only where the completion
/// condition held. Its successor `review` is the state that finishes the
/// ticket, so the edge *out of* `review` is terminal and the edge out of
/// `implement` is not — the difference the selection has to keep straight.
const SELF_ADVANCE_MACHINE: &str = r#"name: self-advance
version: 1
states:
  implement:
    initial: true
    description: Build the change
    target: "fake:acme:model-a"
    snapshot:
      emit:
        name: impl
        on: success
  review:
    description: Review the change
    target: "fake:acme:model-a"
  completed:
    final: true
    description: Done
transitions:
  - from: implement
    to: review
  - from: review
    to: completed
"#;

const ONE_TASK_PLAN: &str = r#"# Rhei: Self-advancing agent

## Tasks

### Task 1: Build
**State:** implement
"#;

/// A worker that leaves `implement` by its own hand.
///
/// `rhei transition` moves the ticket to `review` and the worker exits 0 having
/// written no result, because it is not finishing the ticket — `review` is the
/// state that does that, and it writes one.
const SELF_ADVANCING_AGENT: &str = r#"session_dir = ''
args = sys.argv[1:]
while args:
    flag = args.pop(0)
    if flag == '--session-dir':
        session_dir = args.pop(0) if args else ''
    elif flag in ('--prompt', '--model', '--resume'):
        if args:
            args.pop(0)

state = env('RHEI_STATE')
task = env('RHEI_TASK_ID')

if session_dir:
    session_id = '{}-{}-{}'.format(task, state, env('RHEI_TARGET_SLUG', 'target'))
    write(
        pathlib.Path(session_dir) / (session_id + '.jsonl'),
        '{"session":{"provider":"acme","model":"model-a"}}\n'
        '{"role":"assistant","content":"work"}\n',
    )

if state.startswith('implement'):
    import subprocess

    subprocess.run(
        [
            RHEI_BIN,
            '--state-machine',
            env('RHEI_STATE_MACHINE_PATH'),
            'transition',
            env('RHEI_PLAN_PATH'),
            '--task',
            task,
            '--from',
            'implement',
            '--to',
            'review',
            '--no-callbacks',
        ],
        check=True,
    )
    sys.exit(0)

result('## Result\n\nReviewed.\n')
"#;

fn write_self_advancing_agent(dir: &Path) -> PathBuf {
    // The binary path is JSON-encoded rather than pasted: on Windows it is
    // full of backslashes, which a plain Python literal would read as escapes.
    let body = SELF_ADVANCING_AGENT.replace(
        "RHEI_BIN",
        &serde_json::to_string(&rhei_binary().to_string_lossy()).expect("binary path json"),
    );
    write_python_agent(dir, "self-advancing-agent.py", &body)
}

/// The regression this pins: selecting the exit's edge from the *post-exit*
/// task node moved the state the edge leaves as well as the graph its operands
/// read. A worker that advanced itself into `review` was then judged against
/// `review -> completed`, which is terminal, so the ticket's result was demanded
/// of a visit that never claimed to finish it, the visit was recorded a failure,
/// and the `on: success` snapshot it owed the states after it was never emitted.
// §FS-rhei-agents.3.2
#[test]
fn an_agent_that_advances_its_own_ticket_has_its_visit_recorded_as_a_success() {
    let dir = unique_temp_dir("self-advancing-agent");
    let agent = write_self_advancing_agent(&dir);
    write_fake_snapshot_settings(&dir, &agent);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", SELF_ADVANCE_MACHINE);

    let run = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", run.stdout, run.stderr);

    // The worker moved its own ticket; without that nothing below is a test.
    assert!(
        combined.contains("Task plan.1 advanced: 'implement' -> 'review'"),
        "the worker advances its own ticket:\n{combined}"
    );

    // The edge out of `implement` is not terminal, so condition (3) is vacuous
    // and no result is asked of the visit. §FS-rhei-agents.3.2
    assert!(
        !combined.contains("required outputs are missing for task plan.1"),
        "a self-advancing visit is not asked for the terminal result; got:\n{combined}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    // The `on: success` snapshot is the run's own record of the visit: it is
    // emitted only where the completion condition held, so its presence — and
    // the `completion` it carries — is what says the visit passed.
    let plan_arg = plan_path.to_string_lossy().into_owned();
    let listed = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&listed);
    let rows: serde_json::Value =
        serde_json::from_str(&listed.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "implement"
                && row["completion"] == "success"
        }),
        "the self-advancing visit is recorded a success and emits its snapshot; got:\n{}",
        listed.stdout
    );
}
