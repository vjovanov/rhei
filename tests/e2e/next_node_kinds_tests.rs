//! `rhei next` against a directory workspace that declares its own node kinds.
//!
//! A workspace index declares the heading keywords its task files may use
//! (§FS-rhei-plan-language.3.7). Claim mode re-reads the selected task's file
//! under the lock before writing `**Assignee:**`, and that re-read parses under
//! the kinds the workspace declared rather than the omitted-`structure` default
//! (§FS-rhei-next.3.1).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// A machine whose initial `pending` advances into an agent-bearing `review`,
/// so a successful claim writes a known assignee.
const CLAIM_MACHINE: &str = r#"name: custom-node-kinds
version: 1
states:
  pending:
    initial: true
    description: Ready to claim
  review:
    description: Claimed work
    agent: codex
    instructions: Work the ticket.
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#;

/// Lay out a directory workspace: `index.rhei.md`, one task file under
/// `tasks/`, and a `states.yaml` beside the workspace rather than inside it.
fn workspace_with_task(prefix: &str, index: &str, task: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    fs::write(tasks_dir.join("01-ticket.md"), task).expect("write task file");
    let machine_path = write_fixture_file(&dir, "states.yaml", CLAIM_MACHINE);
    (dir, ws, machine_path)
}

fn task_file(workspace: &Path) -> PathBuf {
    workspace.join("tasks").join("01-ticket.md")
}

/// The defect in agent-grounds/rhei#198: selection finds the custom-kind root,
/// and the claim-time re-read then fails to find it because it parses the task
/// file as though `structure.nodeKinds` were never declared.
// §FS-rhei-next.3.1: the re-read under the lock uses the declared node kinds.
#[test]
fn next_claims_a_custom_kind_root_in_a_directory_workspace() {
    let index = r#"# Rhei: Custom Kinds Directory Workspace
---
structure:
  maxLevels: 2
  nodeKinds: [ticket, step]
---
"#;
    let task = r#"### Ticket ticket: Claimable custom-kind root
**State:** pending

#### Step ticket.triage: Already terminal child
**State:** completed
"#;
    let (_dir, ws, machine_path) = workspace_with_task("next-custom-kind-claim", index, task);

    // Selection is not in question: peek names the root before anything is
    // written, so a failure below is the claim and not the scan.
    let peek = run_cli("next", &ws, &machine_path, &["--no-callbacks", "--peek"]);
    assert_success(&peek);
    assert!(
        peek.stdout.contains("Task workspace.ticket"),
        "peek should name the custom-kind root; got:\n{}",
        peek.stdout
    );

    let result = run_cli("next", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    let content = fs::read_to_string(task_file(&ws)).expect("read task file");
    assert!(
        content.contains(
            "### Ticket ticket: Claimable custom-kind root\n**State:** review\n**Assignee:** codex"
        ),
        "the custom-kind root should be claimed; got:\n{content}"
    );
    let index_content = fs::read_to_string(ws.join("index.rhei.md")).expect("read index");
    assert!(
        !index_content.contains("**Assignee:**"),
        "workspace index must not receive the task assignee; got:\n{index_content}"
    );
}

/// The other half of the same rule: a workspace that declares no kinds still
/// accepts `Task` alone, so threading the declaration through the claim path
/// must not let a custom kind into a plan that never declared one.
// §FS-rhei-plan-language.3.7: with `structure.nodeKinds` omitted, only `Task`.
#[test]
fn next_refuses_a_custom_kind_when_the_workspace_declares_none() {
    let index = "# Rhei: Default Kinds Directory Workspace\n";
    let task = r#"### Task 1: Declares no kinds
**State:** pending

#### Ticket 1.1: Undeclared kind
**State:** completed
"#;
    let (_dir, ws, machine_path) = workspace_with_task("next-default-kind-refusal", index, task);

    let result = run_cli("next", &ws, &machine_path, &["--no-callbacks"]);
    assert!(
        !result.status.success(),
        "an undeclared heading keyword must not be claimable\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "unknown node kind `Ticket` at heading");
    assert_stderr_contains(&result, "this plan declares [\"task\"]");
    let content = fs::read_to_string(task_file(&ws)).expect("read task file");
    assert!(
        !content.contains("**Assignee:**"),
        "nothing should be claimed in a plan that fails to parse; got:\n{content}"
    );
}
