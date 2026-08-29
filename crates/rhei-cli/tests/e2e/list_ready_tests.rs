// `rhei list --ready` against the set `rhei next` draws from: one definition of
// readiness, asked from two surfaces, plus the `--blocked` complement. What
// `rhei next` does once it has picked a ticket lives next door in
// `next_tests.rs`.

// §AR-source-file-size.3 §FS-rhei-list.3.1

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::next_tests::{PARENT_WITH_ONE_OPEN_CHILD, PARENT_WITH_TERMINAL_SUBTREE};
use super::*;

const ONE_TASK_PLAN: &str = r#"# Rhei: Solo

## Tasks

### Task 1: Login
**State:** pending
"#;

/// A machine whose only non-terminal state cannot start until a brief exists.
/// `optional` flips the single bit this pair of cases turns on.
// §FS-rhei-states.3
fn input_machine(optional: bool) -> String {
    let optional_line = if optional { "        optional: true\n" } else { "" };
    format!(
        r#"name: input-machine
version: 1
states:
  pending:
    description: Needs a brief
    initial: true
    inputs:
      - name: brief
        path: runtime/{{task_id}}.md
{optional_line}  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
"#
    )
}

/// The plan, the machine, and the directory the brief would live in.
fn setup_input_plan(prefix: &str, machine: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    (dir, plan_path, machine_path)
}

/// The ids `rhei list --json` printed, in listing order.
fn listed_ids(result: &CliRun) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(&result.stdout)
        .expect("parse JSON")
        .as_array()
        .expect("array")
        .iter()
        .map(|task| task["id"].as_str().expect("id").to_string())
        .collect()
}

/// The issue's own reproduction: a state that requires a brief, and no brief.
///
/// `--ready` judged terminality, gating, priors and the supervision barrier and
/// never opened the filesystem, so it printed a ticket both `rhei next` and
/// `rhei run` refused — on the surface an operator checks first.
// §FS-rhei-list.3.1 §FS-rhei-next.3 §FS-rhei-states.3
#[test]
fn list_ready_and_next_agree_when_a_required_input_is_missing() {
    let (dir, plan_path, machine_path) =
        setup_input_plan("list-ready-missing-input", &input_machine(false));

    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    assert!(listed_ids(&ready).is_empty(), "no brief on disk, so nothing is ready");
    let peek = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!peek.status.success(), "and `rhei next` refuses it too");

    fs::create_dir_all(dir.join("runtime")).expect("create runtime dir");
    fs::write(dir.join("runtime").join("plan.1.md"), "the brief").expect("write brief");

    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    assert_eq!(listed_ids(&ready), vec!["plan.1".to_string()], "the brief exists, so it is ready");
    let peek = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert_success(&peek);
    assert!(peek.stdout.contains("Task plan.1"), "got:\n{}", peek.stdout);
}

/// An `optional: true` input is not part of readiness, exactly as it is not for
/// the scheduler — so widening `--ready` must not narrow it here.
// §FS-rhei-list.3.1 §FS-rhei-states.3
#[test]
fn list_ready_keeps_a_ticket_whose_only_missing_input_is_optional() {
    let (_dir, plan_path, machine_path) =
        setup_input_plan("list-ready-optional-input", &input_machine(true));

    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    assert_eq!(listed_ids(&ready), vec!["plan.1".to_string()], "an optional input blocks nothing");
    assert_success(&run_cli("next", &plan_path, &machine_path, &["--peek"]));
}

/// `--blocked` is the complement of `--ready`, so the ticket waiting on a brief
/// is named by exactly one of them at a time. Before this it was named by
/// neither: `--blocked` answered only the `**Prior:**` question.
// §FS-rhei-list.3.1
#[test]
fn list_blocked_names_the_ticket_whose_required_input_is_missing() {
    let (dir, plan_path, machine_path) =
        setup_input_plan("list-blocked-missing-input", &input_machine(false));

    let blocked = run_cli("list", &plan_path, &machine_path, &["--blocked", "--json"]);
    assert_success(&blocked);
    assert_eq!(
        listed_ids(&blocked),
        vec!["plan.1".to_string()],
        "the missing brief is why it is not moving"
    );

    fs::create_dir_all(dir.join("runtime")).expect("create runtime dir");
    fs::write(dir.join("runtime").join("plan.1.md"), "the brief").expect("write brief");

    let blocked = run_cli("list", &plan_path, &machine_path, &["--blocked", "--json"]);
    assert_success(&blocked);
    assert!(listed_ids(&blocked).is_empty(), "and it stops being blocked once the brief lands");
}

/// A `poll:` state whose next attempt is still ahead is the other condition
/// `--ready` never asked about. The retry deadline is wall-clock state, so the
/// listing has to read it the way the scan does.
// §FS-rhei-list.3.1 §FS-rhei-run.3
#[test]
fn list_ready_and_next_agree_while_a_poll_deadline_is_still_ahead() {
    let machine = r#"name: poll-machine
version: 1
states:
  pending:
    description: Waiting on a remote check
    initial: true
    poll:
      interval: 5m
      max_attempts: 5
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: pending
  - from: pending
    to: completed
"#;
    let deadline = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_secs()
        + 86_400;
    let plan = format!(
        r#"# Rhei: Solo

---
metadata:
  tasks:
    1:
      pollNextAttemptAt:
        pending: {deadline}
---

## Tasks

### Task 1: Login
**State:** pending
"#
    );

    let dir = unique_temp_dir("list-ready-poll-deadline");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    assert!(listed_ids(&ready).is_empty(), "the retry deadline has not come round");

    let blocked = run_cli("list", &plan_path, &machine_path, &["--blocked", "--json"]);
    assert_success(&blocked);
    assert_eq!(
        listed_ids(&blocked),
        vec!["plan.1".to_string()],
        "waiting on a poll deadline is being blocked"
    );

    let peek = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!peek.status.success(), "and `rhei next` refuses it too");
}

/// `rhei list --ready` answers "what could be picked up", so it tracks the same
/// eligibility rule: a parent appears only once its subtree is terminal.
// §FS-rhei-list.3.1
#[test]
fn list_ready_admits_a_parent_only_once_its_subtree_is_terminal() {
    let (open_dir, open_plan, machine_path) =
        setup_single_file("list-ready-open", PARENT_WITH_ONE_OPEN_CHILD);
    let open = run_cli("list", &open_plan, &machine_path, &["--ready", "--json"]);
    assert_success(&open);
    assert_eq!(listed_ids(&open), vec!["plan.1.2".to_string()], "only the open child is ready");
    fs::remove_dir_all(open_dir).expect("cleanup");

    let (done_dir, done_plan, machine_path) =
        setup_single_file("list-ready-closed", PARENT_WITH_TERMINAL_SUBTREE);
    let done = run_cli("list", &done_plan, &machine_path, &["--ready", "--json"]);
    assert_success(&done);
    assert_eq!(
        listed_ids(&done),
        vec!["plan.1".to_string()],
        "the parent is ready once its subtree closes"
    );
    fs::remove_dir_all(done_dir).expect("cleanup");
}

/// `--ready` reports readiness; `rhei next` claims *availability*. The two
/// surfaces answer the same ready-set scan and then differ on purpose: `next`
/// narrows it to a ticket nobody holds and that sits in its machine's initial
/// state. Both narrowings are pinned here, because the help text once claimed
/// `--ready` was "tasks `rhei next` could claim" and both rows disprove it.
// §FS-rhei-list.3.1 §FS-rhei-next.3
#[test]
fn list_ready_lists_tickets_next_refuses_by_assignee_and_by_state() {
    let (_dir, plan_path, machine_path) = setup_single_file(
        "list-ready-vs-next",
        r#"# Rhei: Solo

## Tasks

### Task 1: Claimed already
**State:** draft
**Assignee:** someone

### Task 2: Past its initial state
**State:** pending
"#,
    );

    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    assert_eq!(
        listed_ids(&ready),
        vec!["plan.1".to_string(), "plan.2".to_string()],
        "both are ready: nothing about an assignee or a mid-workflow state blocks work"
    );

    let peek = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!peek.status.success(), "and `rhei next` claims neither: {}", peek.stderr);

    // Neither is blocked either — `--blocked` is the complement of `--ready`,
    // not of "what `rhei next` would hand out". §FS-rhei-list.3.1
    let blocked = run_cli("list", &plan_path, &machine_path, &["--blocked", "--json"]);
    assert_success(&blocked);
    assert!(listed_ids(&blocked).is_empty(), "a ticket `next` refuses is not thereby blocked");

    // The composition §FS-rhei-list.3.1 points at for the narrower question.
    let unclaimed =
        run_cli("list", &plan_path, &machine_path, &["--ready", "--no-assignee", "--json"]);
    assert_success(&unclaimed);
    assert_eq!(listed_ids(&unclaimed), vec!["plan.2".to_string()], "assignees filter separately");
}

/// A ticket in a state no machine declares is not ready. `rhei list` loads
/// leniently and never validates, so it is the only surface that reaches such a
/// ticket at all — `rhei next` and `rhei run` refuse the whole plan. Nothing can
/// be said about readiness in a state that does not exist, so it is blocked;
/// before this it was listed as ready.
// §FS-rhei-list.3.1
#[test]
fn list_reports_a_ticket_in_an_undeclared_state_as_blocked() {
    let (_dir, plan_path, machine_path) = setup_single_file(
        "list-ready-undeclared-state",
        r#"# Rhei: Solo

## Tasks

### Task 1: Misspelled state
**State:** darft

### Task 2: Real state
**State:** draft
"#,
    );

    let ready = run_cli("list", &plan_path, &machine_path, &["--ready", "--json"]);
    assert_success(&ready);
    assert_eq!(
        listed_ids(&ready),
        vec!["plan.2".to_string()],
        "only the ticket whose state the machine declares"
    );

    let blocked = run_cli("list", &plan_path, &machine_path, &["--blocked", "--json"]);
    assert_success(&blocked);
    assert_eq!(
        listed_ids(&blocked),
        vec!["plan.1".to_string()],
        "a state that means nothing is why this one is not moving"
    );

    let peek = run_cli("next", &plan_path, &machine_path, &["--peek"]);
    assert!(!peek.status.success(), "and `rhei next` refuses the plan outright");
    assert!(peek.stderr.contains("darft"), "naming the state it cannot resolve: {}", peek.stderr);
}

/// A machine with one of each reason a ticket stops: a human gate, a supervisor
/// that holds its subtree, and ordinary work.
// §FS-rhei-supervision.3.2
const PARTITION_MACHINE: &str = r#"name: partition-machine
version: 1
states:
  pending:
    description: Ordinary work
    initial: true
  supervising:
    description: Supervise the subtree
    execute_on: descendant-terminal
    agent: claude-code
    visits: 4
  human-review:
    description: A human decides
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - { from: pending, to: completed }
  - { from: supervising, to: completed, condition: openDescendants < 1 }
  - { from: supervising, to: supervising }
  - { from: pending, to: human-review }
  - { from: human-review, to: completed }
"#;

const PARTITION_PLAN: &str = r#"# Rhei: Partition

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Supervise the hardening
**State:** supervising

#### Task 1.1: Held child
**State:** pending

### Task 2: Awaiting a human
**State:** human-review

### Task 3: Parent with an open child
**State:** pending

#### Task 3.1: The open child
**State:** pending

### Task 4: Finished
**State:** completed
"#;

/// The property `--blocked` was widened for: every non-terminal ticket is
/// `--ready` or `--blocked`, never both and never neither. Asserted over a plan
/// carrying each reason at once, so "why is this not moving?" is answerable from
/// one flag whether the answer is a gate, a supervisor, or an open subtree.
// §FS-rhei-list.3.1
#[test]
fn ready_and_blocked_partition_every_non_terminal_ticket() {
    let dir = unique_temp_dir("list-ready-partition");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PARTITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", PARTITION_MACHINE);

    let list = |flag: &str| {
        let result = run_cli("list", &plan_path, &machine_path, &[flag, "--json"]);
        assert_success(&result);
        listed_ids(&result)
    };
    let non_terminal = list("--non-terminal");
    let ready = list("--ready");
    let blocked = list("--blocked");

    // The reasons this plan is worth partitioning: the supervisor is work while
    // its subtree is open, the child it holds is not, and neither is the gate or
    // the parent whose own subtree is still open. §FS-rhei-supervision.3.2
    assert_eq!(ready, vec!["plan.1".to_string(), "plan.3.1".to_string()], "the ready half");
    assert_eq!(
        blocked,
        vec!["plan.1.1".to_string(), "plan.2".to_string(), "plan.3".to_string()],
        "the blocked half: held by a supervisor, waiting on a human, subtree still open"
    );

    let mut union = ready.clone();
    union.extend(blocked.iter().cloned());
    union.sort();
    let mut expected = non_terminal.clone();
    expected.sort();
    assert_eq!(union, expected, "every non-terminal ticket is named by exactly one flag");
    assert!(
        ready.iter().all(|id| !blocked.contains(id)),
        "and none is named by both: ready {ready:?}, blocked {blocked:?}"
    );
}

/// A Panta project of two member rheis, each holding one ticket, with the
/// machine beside it. Returns `(dir, project_root, machine_path)`.
fn setup_two_member_project(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("project");
    for (member, ticket) in [
        ("auth", "### Task 1: Login\n**State:** pending\n"),
        ("billing", "### Task 1: Invoice\n**State:** pending\n"),
    ] {
        fs::create_dir_all(project.join(member).join("tasks")).expect("create member dirs");
        fs::write(project.join(member).join("index.rhei.md"), format!("# Rhei: {member}\n\n"))
            .expect("write member index");
        fs::write(project.join(member).join("tasks").join("ticket.md"), ticket)
            .expect("write member ticket");
    }
    fs::write(project.join("index.panta.md"), "# Panta: Suite\n").expect("write panta manifest");
    let machine_path = write_fixture_file(&dir, "states.yaml", &input_machine(false));
    (dir, project, machine_path)
}

/// A member's required `inputs:` resolve at that member's own execution root,
/// not the enclosing project's — the rule `rhei run` was fixed for in #101, now
/// reachable from `list` because `--ready` is handed the same per-ticket roots.
/// Asserted against `run --dry-run` on both sides, so a listing that agreed by
/// looking in the wrong place would still be caught.
// §AR-rhei-panta.5 §FS-rhei-list.3.1
#[test]
fn list_ready_resolves_a_panta_member_input_at_its_own_root() {
    let (_dir, project, machine_path) = setup_two_member_project("list-ready-panta-member");
    let member_input = project.join("auth").join("runtime").join("auth.1.md");
    let project_input = project.join("runtime").join("auth.1.md");
    fs::create_dir_all(member_input.parent().expect("member runtime")).expect("mkdir member");
    fs::write(&member_input, "the brief").expect("write the brief at the member root");

    let ready = || {
        let result = run_cli("list", &project, &machine_path, &["--ready", "--json"]);
        assert_success(&result);
        listed_ids(&result)
    };
    // `run --dry-run` is the second opinion: it schedules from the same scan.
    let would_schedule = |run: &CliRun| run.stdout.contains("would transition: Task auth.1");

    assert_eq!(ready(), vec!["auth.1".to_string()], "the member's own root is where its brief is");
    let dry_run = run_cli("run", &project, &machine_path, &["--dry-run"]);
    assert!(would_schedule(&dry_run), "and the run agrees: {}", dry_run.stdout);

    // The same file at the *project* root is not the one this ticket needs.
    fs::create_dir_all(project_input.parent().expect("project runtime")).expect("mkdir project");
    fs::rename(&member_input, &project_input).expect("move the brief up to the project root");

    assert!(ready().is_empty(), "a brief at the project root is not the member's brief");
    let dry_run = run_cli("run", &project, &machine_path, &["--dry-run"]);
    assert!(!would_schedule(&dry_run), "and the run agrees again: {}", dry_run.stdout);
}
