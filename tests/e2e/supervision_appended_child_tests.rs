// Steering by appending, end to end: a supervising visit that appends an open
// child and exits 0 writing no result is judged against the plan as it stands
// *after* that exit, so the child it just appended counts as an open descendant,
// the release self-loop is the edge, and no terminal result is demanded of a
// visit that never claimed to finish its subtree.

// §FS-rhei-supervision.4.1 §FS-rhei-agents.3.2

use super::supervision_tests::{
    assert_state_anywhere, setup_supervision_with_agent, spawn_log, supervision_machine,
};
use super::*;

/// A supervisor with no children at all, so `openDescendants` is 0 before the
/// visit and 1 after it — the window the appended child has to be counted in.
const LONE_SUPERVISOR_PLAN: &str = r#"# Rhei: Steer by appending

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise
**State:** supervising
"#;

/// A supervisor that steers on its first visit and finishes on its second.
///
/// Visit 1 appends one open child and exits 0 writing no result, which is what
/// steering looks like: the subtree is not done, so there is nothing to report.
/// Visit 2 finds the subtree closed and writes the result the terminal edge does
/// genuinely require.
const APPENDING_SUPERVISOR_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {} {}\n'.format(task, state, visit))

if state.startswith('supervising'):
    plan = root / 'plan.rhei.md'
    text = plan.read_text(encoding='utf-8')
    if '#### Task 1.1' not in text:
        write(plan, text.rstrip('\n') + '\n\n#### Task 1.1: Review\n**State:** review\n')
        sys.exit(0)
    result('## Result\n\nSubtree closed.\n')
    sys.exit(0)

if state.startswith('review'):
    write(root / 'runtime' / 'review' / (task + '.md'), 'Findings from {}.\n'.format(task))
result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#;

/// The ticket: the appended child is a descendant at the appending visit's own
/// transition, and the run has to see it there.
///
/// The three assertions are the three the reproducer makes, in its order: the
/// terminal result is not demanded of a steering visit, the release self-loop
/// fires and spends the visit, and the appended child runs in the same run
/// rather than waiting for a second `rhei run` to load a graph that holds it.
// §FS-rhei-supervision.4.1 §FS-rhei-agents.3.2
#[test]
fn a_visit_that_appends_an_open_child_releases_the_subtree_and_runs_it() {
    let (dir, plan_path, machine_path) = setup_supervision_with_agent(
        "supervision-appended-child",
        LONE_SUPERVISOR_PLAN,
        &supervision_machine("child-terminal", "completed"),
        APPENDING_SUPERVISOR_AGENT,
    );

    let run = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", run.stdout, run.stderr);

    // The visit appended the child; without that nothing below is being tested.
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    assert!(plan.contains("#### Task 1.1: Review"), "the visit appends its child:\n{plan}");

    // The edge this exit selects is the release self-loop, not the terminal one,
    // so condition (3) is vacuous and no result is asked of the visit.
    // §FS-rhei-agents.3.2
    assert!(
        !combined.contains("required outputs are missing for task plan.1 in state 'supervising'"),
        "a steering visit is not asked for the terminal result; got:\n{combined}"
    );

    // The self-loop spends a visit, so a second visit at counter 2 is how the
    // first one says it fired — and `plan.1.1` ran between the two, in this run.
    // §FS-rhei-supervision.4.1
    assert_eq!(
        spawn_log(&dir),
        vec![
            "plan.1 supervising 1".to_string(),
            "plan.1.1 review 1".to_string(),
            "plan.1 supervising 2".to_string(),
        ],
        "the appending visit releases, the appended child runs, the supervisor is woken again"
    );

    assert_success(&run);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert_state_anywhere(&plan_path, &machine_path, "1.1", "completed");
}
