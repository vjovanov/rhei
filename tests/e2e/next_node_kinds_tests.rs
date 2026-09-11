//! `rhei next` against a plan that declares its own node kinds.
//!
//! A rhei declares the heading keywords its task files may use
//! (§FS-rhei-plan-language.3.7). Claim mode re-reads the selected task's file
//! under the lock before writing `**Assignee:**`, and that re-read parses under
//! the kinds that rhei declared rather than the omitted-`structure` default
//! (§FS-rhei-next.3.1) — whether they come from a workspace index, a
//! single-file plan's own frontmatter, or, for the basin, the project manifest.

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

/// A single-file rhei is its own metadata file, so the claim-time re-read
/// reaches its declared kinds through the plan's own frontmatter and there is
/// nothing to thread — the branch `claim_node_kinds` takes when the metadata
/// file and the task file are one. Pinned so a change to that branch cannot
/// reintroduce the defect here with nothing turning red.
// §FS-rhei-next.3.1: the re-read under the lock uses the declared node kinds.
#[test]
fn next_claims_a_custom_kind_root_in_a_single_file_plan() {
    let plan = r#"# Rhei: Custom Kinds Single File
---
structure:
  maxLevels: 2
  nodeKinds: [ticket, step]
---

## Tasks

### Ticket ticket: Claimable custom-kind root
**State:** pending

#### Step ticket.triage: Already terminal child
**State:** completed
"#;
    let dir = unique_temp_dir("next-custom-kind-single-file");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CLAIM_MACHINE);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    let content = fs::read_to_string(&plan_path).expect("read plan file");
    assert!(
        content.contains(
            "### Ticket ticket: Claimable custom-kind root\n**State:** review\n**Assignee:** codex"
        ),
        "the single-file custom-kind root should be claimed; got:\n{content}"
    );
}

/// The basin has no index of its own: its rhei is synthesised from the project
/// manifest, so the kinds the claim-time re-read applies are the ones
/// `index.panta.md` declares. This path failed for the same reason the
/// directory workspace did.
// §FS-rhei-next.3.1: a basin ticket takes its kinds from the project manifest.
#[test]
fn next_claims_a_custom_kind_basin_ticket_under_the_project_manifest() {
    let manifest = r#"# Panta: Custom Kinds Project
---
structure:
  maxLevels: 2
  nodeKinds: [ticket, step]
---
"#;
    let ticket = r#"### Ticket loose: Claimable basin capture
**State:** pending

#### Step loose.triage: Already terminal child
**State:** completed
"#;
    let dir = unique_temp_dir("next-custom-kind-basin");
    let project = dir.join("project");
    let basin = project.join("basin");
    fs::create_dir_all(&basin).expect("create basin dir");
    fs::write(project.join("index.panta.md"), manifest).expect("write panta manifest");
    let ticket_file = basin.join("001-loose.md");
    fs::write(&ticket_file, ticket).expect("write basin ticket");
    let machine_path = write_fixture_file(&dir, "states.yaml", CLAIM_MACHINE);

    let peek = run_cli("next", &project, &machine_path, &["--no-callbacks", "--peek"]);
    assert_success(&peek);
    assert!(
        peek.stdout.contains("Task basin.loose"),
        "peek should name the basin ticket; got:\n{}",
        peek.stdout
    );

    let result = run_cli("next", &project, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    let content = fs::read_to_string(&ticket_file).expect("read basin ticket");
    assert!(
        content.contains(
            "### Ticket loose: Claimable basin capture\n**State:** review\n**Assignee:** codex"
        ),
        "the basin ticket should be claimed; got:\n{content}"
    );
}
