//! `rhei new` after the block is rendered: the write itself, the two validation
//! passes it is judged by, the rollback, and the report.
//! §FS-rhei-new.5

use std::fs;

use super::new_tests::{
    assert_failure, empty_project, flattened_output, new_run, project_with_rhei,
};
use super::*;

/// Fail, and say something — matched against the flattened output, because
/// miette wraps its diagnostics at a fixed width.
fn assert_says(result: &CliRun, needle: &str) {
    assert!(!result.status.success(), "command should fail\nstdout:\n{}", result.stdout);
    let said = flattened_output(result);
    assert!(said.contains(needle), "expected {needle:?} in output, got:\n{said}");
}

// ---------------------------------------------------------------------------
// Mode, rollback, and report — §FS-rhei-new.5
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

/// §FS-rhei-new.5.1: the created id has to come back out of the *plan*, not
/// only out of the file. `rhei list` performs the same reload the next command
/// would, which is the only assertion a block landing in dead text fails.
#[test]
fn a_created_ticket_reads_back_out_of_the_reloaded_plan() {
    let dir = project_with_rhei("new-ticket-reload");
    let created = new_run(&["new", "Rotate signing keys", "--under", "auth", "--json"], &dir);
    assert_success(&created);
    let value: serde_json::Value =
        serde_json::from_str(created.stdout.trim()).expect("json output");
    assert_eq!(value["id"], "auth.1");

    let listed = new_run(&["list"], &dir);
    assert_success(&listed);
    assert!(listed.stdout.contains("auth.1"), "got: {}", listed.stdout);
    assert!(listed.stdout.contains("Rotate signing keys"), "got: {}", listed.stdout);
}

// ---------------------------------------------------------------------------
// A create never destroys what it finds — §FS-rhei-new.5.1
// ---------------------------------------------------------------------------

/// A task file's name is derived from the id and the title, so two unrelated
/// tickets can pick the same one. The file already there is authored content.
#[test]
fn refuses_to_write_over_an_existing_task_file() {
    let dir = empty_project("new-write-collision");
    assert_success(&new_run(&["new", "Billing", "--dir"], &dir));
    let occupied = dir.join("billing/tasks/1-dunning.md");
    let notes = "Design notes.\nWeeks of research.\n";
    fs::write(&occupied, notes).expect("notes should be written");

    let result = new_run(&["new", "Dunning", "--under", "billing"], &dir);
    assert_failure(&result, "already exists");
    assert_failure(&result, "--id");
    assert_eq!(fs::read_to_string(&occupied).expect("notes"), notes);
}

/// A block appended after an unterminated code fence lands in dead text: the
/// project still validates, and the ticket is not in it. Only reloading the
/// plan can tell the difference. §FS-rhei-new.5.1
#[test]
fn a_block_the_plan_does_not_read_back_is_rolled_back() {
    let dir = empty_project("new-write-dead-text");
    let plan = dir.join("auth.rhei.md");
    let before = "# Rhei: Auth\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\n\
                  ```\nunterminated fence\n";
    fs::write(&plan, before).expect("plan should be written");
    // The fixture is valid on its own: the parser reads the fence as content.
    assert_success(&new_run(&["validate"], &dir));

    let result = new_run(&["new", "Two", "--under", "auth"], &dir);
    assert_says(&result, "ticket 'auth.2' was written to auth.rhei.md, but reloading the project does not find it there");
    assert_eq!(fs::read_to_string(&plan).expect("plan"), before);
}

// ---------------------------------------------------------------------------
// A create answers for the errors it introduced — §FS-rhei-new.5.2
// ---------------------------------------------------------------------------

/// A project holding one broken rhei and one healthy one. This is the project
/// `rhei new` exists for: something is wrong, and the fix starts with adding
/// work to track it.
fn project_failing_validation(prefix: &str) -> std::path::PathBuf {
    let dir = empty_project(prefix);
    write_fixture_file(
        &dir,
        "broken.rhei.md",
        "# Rhei: Broken\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n**Prior:** 99\n",
    );
    write_fixture_file(&dir, "auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n");
    assert!(
        !new_run(&["validate"], &dir).status.success(),
        "the fixture must already be failing validation"
    );
    dir
}

#[test]
fn a_project_that_was_already_failing_still_takes_a_create() {
    let dir = project_failing_validation("new-write-inherited");
    let result = new_run(&["new", "First", "--under", "auth"], &dir);

    assert_success(&result);
    assert!(result.stdout.contains("Created ticket auth.1"), "got: {}", result.stdout);
    assert!(
        result.stderr.contains("already failing validation before this create"),
        "the pre-existing failure must be named, got: {}",
        result.stderr
    );
    assert!(
        fs::read_to_string(dir.join("auth.rhei.md"))
            .expect("rhei file")
            .contains("### Task 1: First"),
        "the write is kept"
    );
}

#[test]
fn a_create_that_adds_an_error_rolls_back_and_reports_only_that_error() {
    let dir = project_failing_validation("new-write-introduced");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let result = new_run(&["new", "Broken", "--under", "auth", "--prior", "99"], &dir);
    assert_says(&result, "Task auth.1 depends on missing Task auth.99");
    let said = flattened_output(&result);
    assert!(
        !said.contains("Task broken.1"),
        "an error the create did not introduce is not its business:\n{said}"
    );
    assert!(result.stderr.contains("nothing was written"), "got: {}", result.stderr);
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

/// §FS-rhei-new.5.2: a rollback removes the directories the create made, not
/// just the file it wrote.
#[test]
fn rolling_back_a_dir_rhei_removes_the_directory_it_made() {
    let dir = empty_project("new-write-dir-rollback");
    let result = new_run(&["new", "Billing", "--dir", "--states", "nowhere"], &dir);

    assert_says(&result, "no states file declaring it was found");
    assert!(!dir.join("billing/tasks").exists(), "tasks/ must be removed");
    assert!(!dir.join("billing").exists(), "the rhei directory must be removed");
}

// ---------------------------------------------------------------------------
// What it prints — §FS-rhei-new.5.4
// ---------------------------------------------------------------------------

#[test]
fn dry_run_under_json_emits_json_and_still_writes_nothing() {
    let dir = project_with_rhei("new-write-dry-json");
    let before = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");

    let result = new_run(&["new", "First", "--under", "auth", "--dry-run", "--json"], &dir);
    assert_success(&result);
    let value: serde_json::Value =
        serde_json::from_str(result.stdout.trim()).expect("--dry-run --json must emit JSON");
    assert_eq!(value["kind"], "ticket");
    assert_eq!(value["id"], "auth.1");
    assert_eq!(value["state"], "pending");
    assert_eq!(value["dry_run"], true);
    assert!(
        value["markdown"].as_str().expect("markdown").contains("### Task 1: First"),
        "got: {value}"
    );
    assert_eq!(fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file"), before);
}

// ---------------------------------------------------------------------------
// The file's own bytes — §FS-rhei-new.3.1
// ---------------------------------------------------------------------------

#[test]
fn a_crlf_plan_stays_crlf_through_an_append_and_an_insert() {
    let dir = empty_project("new-write-crlf");
    let plan = dir.join("auth.rhei.md");
    fs::write(
        &plan,
        "# Rhei: Auth\r\n\r\n## Tasks\r\n\r\n### Task 1: One\r\n**State:** pending\r\n",
    )
    .expect("plan should be written");

    assert_success(&new_run(&["new", "Two", "--under", "auth"], &dir));
    assert_success(&new_run(&["new", "Nested", "--under", "auth.1"], &dir));

    let after = fs::read_to_string(&plan).expect("plan");
    assert_eq!(
        after.matches("\r\n").count(),
        after.matches('\n').count(),
        "a pure CRLF file must stay pure CRLF:\n{after:?}"
    );
    assert_eq!(
        after,
        "# Rhei: Auth\r\n\r\n## Tasks\r\n\r\n### Task 1: One\r\n**State:** pending\r\n\r\n\
         #### Task 1.1: Nested\r\n**State:** pending\r\n\r\n\
         ### Task 2: Two\r\n**State:** pending\r\n"
    );
}

/// Inserting adds lines and removes none: authored spacing is the author's.
#[test]
fn an_insert_adds_lines_and_removes_none() {
    let dir = empty_project("new-write-spacing");
    let plan = dir.join("auth.rhei.md");
    let before = "# Rhei: Auth\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\n\
                  <!-- authored note -->\n\n\n### Task 2: Two\n**State:** pending\n";
    fs::write(&plan, before).expect("plan should be written");

    assert_success(&new_run(&["new", "Nested", "--under", "auth.1"], &dir));

    let after = fs::read_to_string(&plan).expect("plan");
    let inserted = "#### Task 1.1: Nested\n**State:** pending\n\n### Task 2: Two";
    assert_eq!(after, before.replace("### Task 2: Two", inserted));
}

// ---------------------------------------------------------------------------
// The on-ramp: a create is not answerable for the project it found — §FS-rhei-new.5.2
// ---------------------------------------------------------------------------

/// The exact two-step flow `rhei init` advertises, in a project that already
/// carries a dangling `**Prior:**` somewhere else.
///
/// Both steps have to succeed. Diffing rendered error *strings* made step two
/// fail, because the pre-existing error enumerated the project's rhei ids and
/// step one had just added one to that list — a create blamed for rewording an
/// error in a rhei it never touched.
// §FS-rhei-new.5.2
#[test]
fn the_two_step_on_ramp_works_beside_a_broken_sibling() {
    let dir = empty_project("new-on-ramp");
    write_fixture_file(
        &dir,
        "legacy.rhei.md",
        "# Rhei: Legacy\n\n## Tasks\n\n### Task 1: Old thing\n**State:** pending\n\
         **Prior:** nosuch.1\n",
    );

    let rhei = new_run(&["new", "Billing"], &dir);
    assert_success(&rhei);
    assert!(rhei.stdout.contains("--under billing"), "step one must point at step two");

    let ticket = new_run(&["new", "First ticket", "--under", "billing"], &dir);
    assert_success(&ticket);
    assert!(ticket.stdout.contains("Created ticket billing.1"), "got:\n{}", ticket.stdout);

    // Both creates say the inherited failure is not theirs, and keep the write.
    assert!(ticket.stderr.contains("already failing validation"), "got: {}", ticket.stderr);
    assert!(fs::read_to_string(dir.join("billing.rhei.md"))
        .expect("rhei file")
        .contains("First ticket"));
}
