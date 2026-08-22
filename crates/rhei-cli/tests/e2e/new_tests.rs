//! `rhei new` — creating a rhei under Panta, and creating tickets inside one.
//! §FS-rhei-new

use std::fs;

use super::*;

fn new_run(args: &[&str], cwd: &std::path::Path) -> CliRun {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A project directory with `index.panta.md` and nothing else.
fn empty_project(prefix: &str) -> std::path::PathBuf {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "index.panta.md", "# Panta: Test\n");
    dir
}

fn assert_failure(result: &CliRun, needle: &str) {
    assert!(!result.status.success(), "command should fail\nstdout:\n{}", result.stdout);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains(needle), "expected {needle:?} in output, got:\n{combined}");
}

// ---------------------------------------------------------------------------
// Creating a rhei — §FS-rhei-new.2
// ---------------------------------------------------------------------------

#[test]
fn creates_a_single_file_rhei_from_a_title() {
    let dir = empty_project("new-rhei-single");
    let result = new_run(&["new", "Authentication"], &dir);
    assert_success(&result);

    let plan = fs::read_to_string(dir.join("authentication.rhei.md")).expect("rhei file");
    assert_eq!(plan, "# Rhei: Authentication\n\n## Tasks\n");
    assert!(result.stdout.contains("Created rhei \"Authentication\" as `authentication`"));
    // The one command that follows. §FS-rhei-new.5.4
    assert!(result.stdout.contains("--under authentication"));
}

/// §FS-rhei-plan-language.1.1: the empty rhei `new` writes has to validate.
#[test]
fn a_freshly_created_rhei_validates() {
    let dir = empty_project("new-rhei-validates");
    assert_success(&new_run(&["new", "Authentication"], &dir));
    let result = new_run(&["validate"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Validation succeeded"));
}

/// §FS-rhei-new.4: lowercase, non-id characters collapsed to `-`.
#[test]
fn derives_the_id_from_the_title() {
    let dir = empty_project("new-rhei-slug");
    assert_success(&new_run(&["new", "Billing & Dunning"], &dir));
    assert!(dir.join("billing-dunning.rhei.md").is_file());
}

#[test]
fn explicit_id_replaces_derivation() {
    let dir = empty_project("new-rhei-id");
    assert_success(&new_run(&["new", "Authentication", "--id", "auth"], &dir));
    assert!(dir.join("auth.rhei.md").is_file());
}

/// §FS-rhei-new.2.1: `--dir` writes the Directory Workspace shape, whose index
/// must not carry a `## Tasks` section.
#[test]
fn dir_creates_a_workspace_rhei() {
    let dir = empty_project("new-rhei-dir");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));

    let index = fs::read_to_string(dir.join("billing/index.rhei.md")).expect("index");
    assert_eq!(index, "# Rhei: Billing\n");
    assert!(dir.join("billing/tasks").is_dir());
}

/// §FS-rhei-new.2: header fields land in plan-language order — heading,
/// `**States:**`, frontmatter, description, `## Tasks`.
#[test]
fn writes_header_fields_in_plan_language_order() {
    let dir = empty_project("new-rhei-header");
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: custom\nversion: 1\nstates:\n  todo:\n    initial: true\n    description: Todo\n  done:\n    final: true\n    description: Done\ntransitions:\n  - from: todo\n    to: done\n",
    );
    let result = new_run(
        &[
            "new",
            "Billing",
            "--states",
            "custom",
            "--max-levels",
            "3",
            "--node-kinds",
            "task,bug",
            "--description",
            "Everything invoice-related.",
        ],
        &dir,
    );
    assert_success(&result);

    let plan = fs::read_to_string(dir.join("billing.rhei.md")).expect("rhei file");
    assert_eq!(
        plan,
        "# Rhei: Billing\n**States:** custom\n\n---\nstructure:\n  maxLevels: 3\n  \
         nodeKinds: [task, bug]\n---\n\nEverything invoice-related.\n\n## Tasks\n"
    );
}

#[test]
fn refuses_the_reserved_basin_id() {
    let dir = empty_project("new-rhei-basin");
    assert_failure(&new_run(&["new", "Basin", "--id", "basin"], &dir), "reserved rhei id");
}

#[test]
fn refuses_a_colliding_rhei_id() {
    let dir = empty_project("new-rhei-collision");
    assert_success(&new_run(&["new", "Auth"], &dir));
    assert_failure(&new_run(&["new", "Auth"], &dir), "already exists");
}

#[test]
fn refuses_an_illegal_explicit_id() {
    let dir = empty_project("new-rhei-illegal");
    assert_failure(&new_run(&["new", "Auth", "--id", "9lives"], &dir), "not a valid rhei id");
}

/// §FS-rhei-new.2.1: a rhei is a member of a project; a lone plan has nowhere
/// to put a second one.
#[test]
fn refuses_to_create_a_rhei_outside_a_project() {
    let dir = unique_temp_dir("new-rhei-lone");
    write_fixture_file(
        &dir,
        "solo.rhei.md",
        "# Rhei: Solo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n",
    );
    assert_failure(&new_run(&["new", "Another"], &dir), "is not a Panta project");
}

// ---------------------------------------------------------------------------
// Creating a ticket — §FS-rhei-new.3
// ---------------------------------------------------------------------------

fn project_with_rhei(prefix: &str) -> std::path::PathBuf {
    let dir = empty_project(prefix);
    assert_success(&new_run(&["new", "Authentication", "--id", "auth"], &dir));
    dir
}

#[test]
fn creates_a_top_level_ticket_numbered_from_its_siblings() {
    let dir = project_with_rhei("new-ticket-top");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    let result = new_run(&["new", "Second", "--under", "auth"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket auth.2"));

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(plan.contains("### Task 1: First\n**State:** pending\n"));
    assert!(plan.contains("### Task 2: Second\n**State:** pending\n"));
}

/// §FS-rhei-new.1.3: every plan-language metadata field has a flag.
#[test]
fn writes_every_metadata_field_in_grammar_order() {
    let dir = project_with_rhei("new-ticket-fields");
    assert_success(&new_run(&["new", "First", "--under", "auth", "--provides", "api"], &dir));
    let result = new_run(
        &[
            "new",
            "Second",
            "--under",
            "auth",
            "--prior",
            "1",
            "--provides",
            "client",
            "--consumes",
            "1:api",
            "--assignee",
            "vj",
            "--description",
            "Body text.",
        ],
        &dir,
    );
    assert_success(&result);

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(
        plan.contains(
            "### Task 2: Second\n**State:** pending\n**Prior:** 1\n**Provides:** client\n\
             **Consumes:** 1:api\n**Assignee:** vj\n\nBody text.\n"
        ),
        "metadata order wrong:\n{plan}"
    );
}

/// §FS-rhei-new.3.1: a subtask goes after its parent's subtree, not at the end
/// of the file.
#[test]
fn inserts_a_subtask_after_its_parents_subtree() {
    let dir = project_with_rhei("new-ticket-subtask");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Second", "--under", "auth"], &dir));
    let result = new_run(&["new", "Nested", "--under", "auth.1"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket auth.1.1"));

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    let nested = plan.find("#### Task 1.1: Nested").expect("subtask present");
    let second = plan.find("### Task 2: Second").expect("sibling present");
    assert!(nested < second, "subtask must precede the next top-level ticket:\n{plan}");
}

#[test]
fn workspace_tickets_get_their_own_task_file_and_subtasks_join_it() {
    let dir = empty_project("new-ticket-workspace");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    assert_success(&new_run(&["new", "Dunning emails", "--under", "billing"], &dir));
    assert_success(&new_run(&["new", "Retry schedule", "--under", "billing.1"], &dir));

    let task_file = dir.join("billing/tasks/1-dunning-emails.md");
    let contents = fs::read_to_string(&task_file).expect("task file");
    assert!(contents.starts_with("### Task 1: Dunning emails\n"));
    // A task file owns a subtree. §FS-rhei-new.3.1
    assert!(contents.contains("#### Task 1.1: Retry schedule\n"));
    assert_eq!(
        fs::read_dir(dir.join("billing/tasks")).expect("tasks dir").count(),
        1,
        "a subtask must not become a second file"
    );
}

/// §FS-rhei-panta.2: the basin is created on demand and has no authored index.
#[test]
fn captures_into_the_basin_creating_it_on_demand() {
    let dir = project_with_rhei("new-ticket-basin");
    let result = new_run(&["new", "Fix the footer typo", "--under", "basin"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket basin.1"));
    assert!(dir.join("basin/1-fix-the-footer-typo.md").is_file());
    assert!(!dir.join("basin/index.rhei.md").exists(), "the basin manifest is synthetic");
    assert_success(&new_run(&["validate"], &dir));
}

/// §FS-rhei-new.3.2: the state comes from the *owning rhei's* machine, not the
/// project default.
#[test]
fn starts_in_the_owning_rheis_initial_state() {
    let dir = empty_project("new-ticket-state");
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: custom\nversion: 1\nstates:\n  todo:\n    initial: true\n    description: Todo\n  done:\n    final: true\n    description: Done\ntransitions:\n  - from: todo\n    to: done\n",
    );
    assert_success(&new_run(&["new", "Reporting", "--states", "custom"], &dir));
    let result = new_run(&["new", "First report", "--under", "reporting"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("[todo]"), "got: {}", result.stdout);
}

#[test]
fn explicit_state_is_checked_against_that_machine() {
    let dir = project_with_rhei("new-ticket-badstate");
    assert_failure(
        &new_run(&["new", "First", "--under", "auth", "--state", "nope"], &dir),
        "has no state 'nope'",
    );
}

#[test]
fn refuses_an_undeclared_node_kind() {
    let dir = project_with_rhei("new-ticket-kind");
    assert_failure(
        &new_run(&["new", "First", "--under", "auth", "--kind", "spike"], &dir),
        "does not declare the node kind 'spike'",
    );
}

/// §FS-rhei-new.3.3: depth is refused before anything is written.
#[test]
fn refuses_a_subtask_deeper_than_max_levels() {
    let dir = project_with_rhei("new-ticket-depth");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Nested", "--under", "auth.1"], &dir));
    assert_failure(&new_run(&["new", "Deeper", "--under", "auth.1.1"], &dir), "allows 2");
}

#[test]
fn refuses_an_unknown_parent() {
    let dir = project_with_rhei("new-ticket-parent");
    assert_failure(&new_run(&["new", "First", "--under", "nope"], &dir), "names no rhei or ticket");
}

// ---------------------------------------------------------------------------
// Shared behavior — §FS-rhei-new.5
// ---------------------------------------------------------------------------

/// §FS-rhei-new.5.3: a flag belonging to the other mode is refused, never
/// silently ignored.
#[test]
fn refuses_flags_from_the_other_mode() {
    let dir = project_with_rhei("new-mode");
    assert_failure(
        &new_run(&["new", "X", "--under", "auth", "--dir"], &dir),
        "--dir configures a new rhei",
    );
    assert_failure(
        &new_run(&["new", "X", "--state", "pending"], &dir),
        "--state configures a new ticket",
    );
}

/// §FS-rhei-new.5.2: a create that would not validate is undone.
#[test]
fn rolls_back_a_create_that_fails_validation() {
    let dir = project_with_rhei("new-rollback");
    assert_success(&new_run(&["new", "First", "--under", "auth"], &dir));
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let result = new_run(&["new", "Broken", "--under", "auth", "--prior", "99"], &dir);
    assert!(!result.status.success(), "an unresolvable prior must fail");
    assert!(result.stderr.contains("nothing was written"), "got: {}", result.stderr);
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

#[test]
fn rollback_removes_a_file_it_created() {
    let dir = empty_project("new-rollback-file");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    let result = new_run(&["new", "Broken", "--under", "billing", "--prior", "99"], &dir);
    assert!(!result.status.success());
    assert_eq!(fs::read_dir(dir.join("billing/tasks")).expect("tasks dir").count(), 0);
}

#[test]
fn keep_on_error_leaves_the_write_in_place() {
    let dir = empty_project("new-keep");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    let result =
        new_run(&["new", "Broken", "--under", "billing", "--prior", "99", "--keep-on-error"], &dir);
    assert!(!result.status.success());
    assert!(result.stderr.contains("left failing validation"), "got: {}", result.stderr);
    assert_eq!(fs::read_dir(dir.join("billing/tasks")).expect("tasks dir").count(), 1);
}

/// §FS-rhei-new.5.4: `--dry-run` prints the block and touches nothing.
#[test]
fn dry_run_writes_nothing() {
    let dir = project_with_rhei("new-dry-run");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    let result = new_run(&["new", "First", "--under", "auth", "--dry-run"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Would create ticket auth.1"));
    assert!(result.stdout.contains("### Task 1: First"));
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

#[test]
fn json_reports_the_created_ticket() {
    let dir = project_with_rhei("new-json");
    let result = new_run(&["new", "First", "--under", "auth", "--json"], &dir);
    assert_success(&result);
    let value: serde_json::Value = serde_json::from_str(result.stdout.trim()).expect("json output");
    assert_eq!(value["kind"], "ticket");
    assert_eq!(value["id"], "auth.1");
    assert_eq!(value["title"], "First");
    assert_eq!(value["state"], "pending");
}

/// §FS-rhei-new.3: a lone plan is its own rhei, so tickets can be added to it
/// even outside a project.
#[test]
fn adds_a_ticket_to_a_lone_plan() {
    let dir = unique_temp_dir("new-ticket-lone");
    write_fixture_file(
        &dir,
        "solo.rhei.md",
        "# Rhei: Solo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n",
    );
    let result = new_run(&["new", "Two", "--under", "solo"], &dir);
    assert_success(&result);
    assert!(result.stdout.contains("Created ticket solo.2"));
}
