use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rhei_core::ast::TaskId;
use rhei_core::parse;
use rhei_core::workspace;
use rhei_output::{to_github_markdown, to_json_value, ProgressReportOutput};
use rhei_validator::{validate_with_machine, StateMachine};

#[allow(dead_code)]
#[path = "../../rhei-core/tests/fixtures.rs"]
mod fixtures;

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("rhei-{prefix}-{nanos}"));
    fs::create_dir_all(&dir).expect("temporary directory should be created");
    dir
}

fn write_fixture_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("fixture file should be written");
    path
}

const CLI_VALID_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Subtask 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

#### Subtask 1.2: Record rollback contract
**State:** completed
```yaml
rollback:
  enabled: true
```

### Task 2: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Subtask 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Task 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task 2

#### Subtask 3.1: Dry run in staging
**State:** pending
Run the bot in dry-run mode against staging.
"#;

const CLI_PRIMARY_ERROR_REGRESSION_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Subtask 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

### Task bootstrap_env: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Subtask 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Tak 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task bootstrap_env

#### Subtask 3.1: Dry run in staging
**State:** pending
Run the bot in dry-run mode against staging.
"#;

struct CliRun {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_validate(plan: &str, machine: &str, prefix: &str) -> CliRun {
    let temp_dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(&temp_dir, "plan.md", plan);
    let machine_path = write_fixture_file(&temp_dir, "states.yaml", machine);

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    let result = CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };

    fs::remove_dir_all(temp_dir).expect("temporary directory should be removed");

    result
}

fn normalize_for_assertions(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_contains_in_order(haystack: &str, fragments: &[&str], context: &str, rendered: &str) {
    let mut search_start = 0usize;

    for fragment in fragments {
        let Some(relative_index) = haystack[search_start..].find(fragment) else {
            panic!("expected {context} fragment {:?} in order, got:\n{}", fragment, rendered);
        };
        search_start += relative_index + fragment.len();
    }
}

fn assert_parse_failure(
    result: &CliRun,
    parser_message_fragments: &[&str],
    line_hint: Option<&str>,
    excerpt: Option<&str>,
    unrelated_messages: &[&str],
) {
    let normalized_stderr = normalize_for_assertions(&result.stderr);

    assert!(
        !result.status.success(),
        "expected parse failure\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        normalized_stderr.contains("PARSE ERROR"),
        "expected Elm-style parse header in stderr, got:\n{}",
        result.stderr
    );
    assert_contains_in_order(
        &normalized_stderr,
        parser_message_fragments,
        "parser message",
        &result.stderr,
    );

    if let Some(line_hint) = line_hint {
        assert!(
            normalized_stderr.contains(&normalize_for_assertions(line_hint)),
            "expected line hint {:?}, got:\n{}",
            line_hint,
            result.stderr
        );
    }

    if let Some(excerpt) = excerpt {
        assert!(
            normalized_stderr.contains(&normalize_for_assertions(excerpt)),
            "expected source excerpt {:?}, got:\n{}",
            excerpt,
            result.stderr
        );
    }

    assert!(
        !normalized_stderr.contains("VALIDATION ERROR"),
        "parse failures should not fall through to validation output, got:\n{}",
        result.stderr
    );

    for unrelated in unrelated_messages {
        assert!(
            !normalized_stderr.contains(&normalize_for_assertions(unrelated)),
            "unexpected unrelated validator noise {:?} in stderr:\n{}",
            unrelated,
            result.stderr
        );
    }
}

fn assert_validation_failure(
    result: &CliRun,
    expected_message_fragments: &[&[&str]],
    absent_messages: &[&str],
) {
    let normalized_stderr = normalize_for_assertions(&result.stderr);

    assert!(
        !result.status.success(),
        "expected validation failure\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        normalized_stderr.contains("VALIDATION ERROR"),
        "expected Elm-style validation header, got:\n{}",
        result.stderr
    );
    assert!(
        !normalized_stderr.contains("PARSE ERROR"),
        "semantic failures should not be labeled as parse failures, got:\n{}",
        result.stderr
    );

    for fragments in expected_message_fragments {
        assert_contains_in_order(
            &normalized_stderr,
            fragments,
            "validation message",
            &result.stderr,
        );
    }

    for message in absent_messages {
        assert!(
            !normalized_stderr.contains(&normalize_for_assertions(message)),
            "unexpected message {:?} in stderr:\n{}",
            message,
            result.stderr
        );
    }
}

#[test]
fn valid_plan_parses_validates_and_renders_across_crates() {
    let rhei = parse(CLI_VALID_PLAN).expect("valid fixture should parse");
    let machine =
        StateMachine::from_yaml_str(fixtures::TEST_STATE_MACHINE).expect("machine should load");
    let report = validate_with_machine(&rhei, &machine);

    assert!(
        !report.has_errors(),
        "expected valid plan fixture to pass validation, got: {:?}",
        report.errors
    );
    assert!(report.warnings.is_empty(), "expected no warnings, got: {:?}", report.warnings);

    assert_eq!(rhei.title, "Release Automation Rollout");
    assert_eq!(rhei.tasks.len(), 3);
    assert_eq!(rhei.tasks[0].id, TaskId::Number(1));
    assert_eq!(rhei.tasks[1].id, TaskId::Number(2));
    assert_eq!(rhei.tasks[2].prior.len(), 2);

    let json = to_json_value(&rhei);
    assert_eq!(json["title"].as_str(), Some("Release Automation Rollout"));
    assert_eq!(json["tasks"].as_array().map(Vec::len), Some(3));

    let github = to_github_markdown(&rhei);
    assert!(github.contains("### Task 1: Define pipeline contracts"));
    assert!(github.contains("### Task 2: Bootstrap environments"));
    assert!(github.contains("- Prior: Task 1, Task 2"));
    assert!(github.contains("- [ ] 3.1: Dry run in staging"));

    let progress = ProgressReportOutput { color: false, show_dependencies: true }.to_string(&rhei);
    assert!(progress.contains("Rhei: Release Automation Rollout"));
    assert!(progress.contains("* Task 2: Bootstrap environments  [IN-PROGRESS]"));
    assert!(progress.contains("  - Prior: Task 1, Task 2"));
}

#[test]
fn invalid_plan_reports_cross_component_validation_failures() {
    let rhei = parse(fixtures::INVALID_PLAN).expect("invalid semantic fixture should still parse");
    let machine =
        StateMachine::from_yaml_str(fixtures::TEST_STATE_MACHINE).expect("machine should load");
    let report = validate_with_machine(&rhei, &machine);

    assert!(report.has_errors(), "expected semantic validation errors");
    let joined = report.errors.join("\n");

    assert!(joined
        .contains("Subtask 2.1 ('Wrong subtask parent') is under Task 1 but declares parent 2"));
    assert!(joined.contains("Circular dependency detected"));
}

#[test]
fn cli_validate_and_render_use_real_fixture_files() {
    let temp_dir = unique_temp_dir("integration-cli");
    let plan_path = write_fixture_file(&temp_dir, "valid-plan.md", CLI_VALID_PLAN);
    let machine_path = write_fixture_file(&temp_dir, "states.yaml", fixtures::TEST_STATE_MACHINE);

    let validate = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    assert!(
        validate.status.success(),
        "validate command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    let validate_stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(validate_stdout.contains("Validation succeeded"));

    let render = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("render")
        .arg(&plan_path)
        .arg("--format")
        .arg("json")
        .arg("--pretty")
        .output()
        .expect("render command should run");

    assert!(
        render.status.success(),
        "render command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&render.stdout),
        String::from_utf8_lossy(&render.stderr)
    );
    let render_stdout = String::from_utf8_lossy(&render.stdout);
    assert!(render_stdout.contains("\"title\": \"Release Automation Rollout\""));
    assert!(render_stdout.contains("\"number\": 2"));

    fs::remove_dir_all(temp_dir).expect("temporary directory should be removed");
}

#[test]
fn cli_validate_surfaces_validation_errors_for_fixture() {
    let temp_dir = unique_temp_dir("integration-cli-invalid");
    let plan_path = write_fixture_file(&temp_dir, "invalid-plan.md", fixtures::INVALID_PLAN);
    let machine_path = write_fixture_file(&temp_dir, "states.yaml", fixtures::TEST_STATE_MACHINE);

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    assert!(!output.status.success(), "invalid plan should fail validation");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("Validation succeeded"),
        "invalid validation should not report success\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stderr.contains("VALIDATION ERROR"));
    assert!(stderr.contains("Circular dependency detected"));

    fs::remove_dir_all(temp_dir).expect("temporary directory should be removed");
}

#[test]
fn cli_validate_reports_missing_rhei_header_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MISSING_RHEI_HEADER,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-missing-rhei",
    );

    assert_parse_failure(
        &result,
        &["Missing", "Rhei:", "header"],
        Some("line 1"),
        Some("## Tasks"),
        &[
            "missing mandatory **State:**",
            "depends on missing Task",
            "Circular dependency detected",
        ],
    );
}

#[test]
fn cli_validate_reports_malformed_rhei_header_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_RHEI_HEADER,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-rhei",
    );

    assert_parse_failure(
        &result,
        &["Malformed rhei heading", "expected", "Rhei:", "title"],
        Some("line 1"),
        Some("#Rhei: Missing required space"),
        &["VALIDATION ERROR", "missing mandatory **State:**"],
    );
}

#[test]
fn cli_validate_reports_h1_typo_as_malformed_rhei_header_parse_failure() {
    let result = run_validate(
        r#"# Sga: Release Automation Rollout

## Tasks

### Task 1: Primary task
**State:** pending
"#,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-rhei-heading-typo",
    );

    assert_parse_failure(
        &result,
        &["Malformed rhei heading", "expected", "Rhei:", "title"],
        Some("line 1"),
        Some("# Sga: Release Automation Rollout"),
        &["VALIDATION ERROR", "missing mandatory **State:**"],
    );
}

#[test]
fn cli_validate_reports_missing_tasks_section_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MISSING_TASKS_SECTION,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-missing-tasks",
    );

    assert_parse_failure(
        &result,
        &["Missing", "Tasks", "section"],
        None,
        None,
        &["missing mandatory **State:**", "Circular dependency detected"],
    );
}

#[test]
fn cli_validate_reports_empty_tasks_section_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_EMPTY_TASKS_SECTION,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-empty-tasks",
    );

    assert_parse_failure(
        &result,
        &["Tasks section", "must contain at least one task"],
        Some("line 3"),
        Some("## Tasks"),
        &["missing mandatory **State:**", "depends on missing Task"],
    );
}

#[test]
fn cli_validate_reports_malformed_task_heading_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_TASK_HEADING,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-task-heading",
    );

    assert_parse_failure(
        &result,
        &["Malformed task heading", "expected", "Task", "title"],
        Some("line 5"),
        Some("### Tak 1: Broken keyword"),
        &["Subtask 1.1", "missing mandatory **State:**"],
    );
}

#[test]
fn cli_validate_reports_malformed_subtask_heading_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_SUBTASK_HEADING,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-subtask-heading",
    );

    assert_parse_failure(
        &result,
        &["Malformed subtask heading", "expected", "Subtask", "title"],
        Some("line 8"),
        Some("#### Subtask 1: Missing decimal component"),
        &["missing mandatory **State:**", "Subtask 1.1"],
    );
}

#[test]
fn cli_validate_reports_malformed_state_metadata_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_STATE_METADATA,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-state-metadata",
    );

    assert_parse_failure(
        &result,
        &["Malformed metadata field", "expected", "State:", "value"],
        Some("line 6"),
        Some("**State** pending"),
        &["missing mandatory **State:**", "invalid state"],
    );
}

#[test]
fn cli_validate_reports_malformed_prior_metadata_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_PRIOR_METADATA,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-prior-metadata",
    );

    assert_parse_failure(
        &result,
        &["Malformed metadata field", "expected", "Prior:", "Task", "id"],
        Some("line 7"),
        Some("**Prior** Task 2"),
        &["depends on missing Task"],
    );
}

#[test]
fn cli_validate_reports_late_metadata_after_content_as_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_LATE_METADATA_AFTER_CONTENT,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-late-metadata",
    );

    assert_parse_failure(
        &result,
        &[
            "Metadata fields",
            "must appear immediately",
            "after the task heading",
            "before task content",
        ],
        Some("line 8"),
        Some("**Prior:** Task 2"),
        &["depends on missing Task", "Circular dependency detected"],
    );
}

#[test]
fn cli_validate_reports_metadata_outside_task_as_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_METADATA_OUTSIDE_TASK,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-metadata-outside-task",
    );

    assert_parse_failure(
        &result,
        &["Metadata field", "appears outside a task"],
        Some("line 3"),
        Some("**State:** pending"),
        &["missing mandatory **State:**", "VALIDATION ERROR"],
    );
}

#[test]
fn cli_validate_reports_missing_state_as_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MISSING_STATE,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-missing-state",
    );

    assert_parse_failure(&result, &["missing mandatory", "State:", "metadata"], None, None, &[]);
}

#[test]
fn cli_validate_reports_prior_before_state_as_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_PRIOR_BEFORE_STATE,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-prior-before-state",
    );

    assert_parse_failure(&result, &["State:", "must appear before", "Prior:"], None, None, &[]);
}

#[test]
fn cli_validate_reports_invalid_state_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_INVALID_STATE,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-invalid-state",
    );

    assert_validation_failure(
        &result,
        &[
            &["Task 1", "has invalid state", "blocked", "Allowed:"],
            &["completed"],
            &["pending"],
            &["in-", "progress"],
        ],
        &["failed to parse", "Malformed metadata field"],
    );
}

#[test]
fn cli_validate_reports_missing_dependency_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MISSING_DEPENDENCY,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-missing-dependency",
    );

    assert_validation_failure(
        &result,
        &[&["Task 1", "depends on missing Task 99"]],
        &["failed to parse", "Malformed task heading"],
    );
}

#[test]
fn cli_validate_reports_circular_dependency_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_CIRCULAR_DEPENDENCY,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-circular-dependency",
    );

    assert_validation_failure(
        &result,
        &[&["Circular dependency detected", "among tasks:"]],
        &["failed to parse", "Malformed task heading"],
    );
}

#[test]
fn cli_validate_reports_subtask_parent_mismatch_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_SUBTASK_PARENT_MISMATCH,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-subtask-parent-mismatch",
    );

    assert_validation_failure(
        &result,
        &[&["Subtask 1.1", "Wrong parent prefix", "under Task 2", "declares", "parent 1"]],
        &["failed to parse", "Malformed subtask heading"],
    );
}

#[test]
fn cli_validate_rejects_named_task_with_subtasks() {
    let plan = r#"# Rhei: Named Subtask Test

## Tasks

### Task build: Build step
**State:** pending

#### Subtask 1.1: Sub
**State:** pending
"#;
    let result =
        run_validate(plan, fixtures::TEST_STATE_MACHINE, "integration-cli-named-subtask-error");

    assert_validation_failure(
        &result,
        &[&["Task 'build'", "named id", "must not declare subtasks"]],
        &["failed to parse"],
    );
}

#[test]
fn malformed_task_heading_reports_parse_error_instead_of_subtask_validation_error() {
    let result = run_validate(
        CLI_PRIMARY_ERROR_REGRESSION_PLAN,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-heading",
    );

    assert_parse_failure(
        &result,
        &["Malformed task heading", "expected", "Task", "title"],
        Some("line 20"),
        Some("### Tak 3: Roll out release bot"),
        &["Subtask 3.1 ('Dry run in staging') is under Task 2 but declares parent 3"],
    );
}

#[test]
fn malformed_state_metadata_reports_parse_error_instead_of_missing_state_validation_error() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_STATE_METADATA,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-state-metadata-regression",
    );

    assert_parse_failure(
        &result,
        &["Malformed metadata field", "expected", "State:", "value"],
        Some("line 6"),
        Some("**State** pending"),
        &["Task 1 is missing mandatory **State:** metadata"],
    );
}

// ---- Transition command integration tests ----

const TRANSITION_STATE_MACHINE: &str = r#"name: transition-test
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task being worked on
  completed:
    description: Task finished
    final: true
  cancelled:
    description: Task abandoned
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
  - from: "*"
    to: cancelled
"#;

const TRANSITION_PLAN: &str = r#"# Rhei: Transition Test

## Tasks

### Task 1: First task
**State:** pending

### Task 2: Second task
**State:** in-progress
**Prior:** Task 1
"#;

fn run_transition(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(machine_path)
        .arg("transition")
        .arg(plan_path)
        .arg("--task")
        .arg(task)
        .arg("--from")
        .arg(from)
        .arg("--to")
        .arg(to)
        .output()
        .expect("transition command should run");

    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn transition_succeeds_and_updates_file() {
    let dir = unique_temp_dir("transition-success");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "1", "pending", "in-progress");

    assert!(
        result.status.success(),
        "transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("pending"),
        "stdout should mention old state; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("in-progress"),
        "stdout should mention new state; got:\n{}",
        result.stdout
    );

    // Verify the file was actually updated.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::Number(1)).expect("Task 1 exists");
    assert_eq!(task1.state.as_str(), "in-progress");

    // Task 2 should be untouched.
    let task2 = rhei.tasks.iter().find(|t| t.id == TaskId::Number(2)).expect("Task 2 exists");
    assert_eq!(task2.state.as_str(), "in-progress");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_fails_on_cas_conflict() {
    let dir = unique_temp_dir("transition-cas");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    // Task 1 is in "pending", but we claim it's "in-progress".
    let result = run_transition(&plan_path, &machine_path, "1", "in-progress", "completed");

    assert!(!result.status.success(), "transition should fail on CAS conflict");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(normalized.contains("conflict"), "should report conflict; got:\n{}", result.stderr);
    assert!(
        normalized.contains("pending"),
        "should mention actual state 'pending'; got:\n{}",
        result.stderr
    );

    // File should be unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, TRANSITION_PLAN);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_fails_on_invalid_transition() {
    let dir = unique_temp_dir("transition-invalid");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    // pending → completed is not a declared transition.
    let result = run_transition(&plan_path, &machine_path, "1", "pending", "completed");

    assert!(!result.status.success(), "transition should fail for disallowed transition");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not allowed"),
        "should report transition not allowed; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_fails_on_nonexistent_task() {
    let dir = unique_temp_dir("transition-missing");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "99", "pending", "in-progress");

    assert!(!result.status.success(), "transition should fail for nonexistent task");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not found"),
        "should report task not found; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_works_with_named_task_id() {
    let plan = r#"# Rhei: Named Task Test

## Tasks

### Task setup: Initialize project
**State:** pending

### Task build: Build artifacts
**State:** pending
**Prior:** Task setup
"#;

    let dir = unique_temp_dir("transition-named");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    let result = run_transition(&plan_path, &machine_path, "setup", "pending", "in-progress");

    assert!(
        result.status.success(),
        "transition should succeed for named task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei
        .tasks
        .iter()
        .find(|t| t.id == TaskId::Named("setup".to_string()))
        .expect("Task setup exists");
    assert_eq!(task.state.as_str(), "in-progress");

    // Task build should be untouched.
    let build = rhei
        .tasks
        .iter()
        .find(|t| t.id == TaskId::Named("build".to_string()))
        .expect("Task build exists");
    assert_eq!(build.state.as_str(), "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_wildcard_from_allows_any_source() {
    let dir = unique_temp_dir("transition-wildcard");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", TRANSITION_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", TRANSITION_STATE_MACHINE);

    // The wildcard `from: "*"` → cancelled should allow pending → cancelled.
    let result = run_transition(&plan_path, &machine_path, "1", "pending", "cancelled");

    assert!(
        result.status.success(),
        "wildcard transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::Number(1)).expect("Task 1 exists");
    assert_eq!(task1.state.as_str(), "cancelled");

    fs::remove_dir_all(dir).expect("cleanup");
}

// --- Callback execution integration tests ---

const CALLBACK_STATE_MACHINE: &str = r#"name: callback-test
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "cli:echo on_leave_fired"
    on_enter: "cli:echo on_enter_fired"
  - from: in-progress
    to: completed
    on_leave: "cli:exit 1"
"#;

fn run_transition_with_flags(
    plan_path: &Path,
    machine_path: &Path,
    task: &str,
    from: &str,
    to: &str,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.arg("--state-machine")
        .arg(machine_path)
        .arg("transition")
        .arg(plan_path)
        .arg("--task")
        .arg(task)
        .arg("--from")
        .arg(from)
        .arg("--to")
        .arg(to);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("transition command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn callback_on_leave_and_on_enter_invoked_on_transition() {
    let dir = unique_temp_dir("callback-invoked");
    let plan = r#"# Rhei: Callback Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CALLBACK_STATE_MACHINE);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        result.status.success(),
        "transition with callbacks should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Verify the file was updated.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::Number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "in-progress");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_on_leave_failure_blocks_transition() {
    let dir = unique_temp_dir("callback-blocks");
    let plan = r#"# Rhei: Callback Failure Test

## Tasks

### Task 1: Alpha
**State:** in-progress
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CALLBACK_STATE_MACHINE);

    // in-progress → completed has on_leave: "cli:exit 1" which should fail.
    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "in-progress", "completed", &[]);

    assert!(!result.status.success(), "transition should fail when on_leave callback rejects");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("on_leave") && normalized.contains("rejected"),
        "should report on_leave rejection; got:\n{}",
        result.stderr
    );

    // File should be unchanged — transition did not proceed.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn no_callbacks_flag_skips_callback_execution() {
    let dir = unique_temp_dir("callback-skip");
    let plan = r#"# Rhei: No Callbacks Test

## Tasks

### Task 1: Alpha
**State:** in-progress
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", CALLBACK_STATE_MACHINE);

    // on_leave would fail (exit 1), but --no-callbacks should skip it.
    let result = run_transition_with_flags(
        &plan_path,
        &machine_path,
        "1",
        "in-progress",
        "completed",
        &["--no-callbacks"],
    );

    assert!(
        result.status.success(),
        "transition with --no-callbacks should succeed even when callback would fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::Number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_unknown_platform_produces_clear_error() {
    let machine_yaml = r#"name: bad-callback
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
transitions:
  - from: pending
    to: in-progress
    on_leave: "js:someFunction"
"#;
    let dir = unique_temp_dir("callback-bad-platform");
    let plan = r#"# Rhei: Bad Callback Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(!result.status.success(), "transition should fail for unknown callback platform");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("unknown callback platform"),
        "should report unknown platform; got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("js:someFunction"),
        "should include the callback identifier; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

// ---- Run command integration tests ----

const RUN_STATE_MACHINE: &str = r#"name: run-test
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task being worked on
  completed:
    description: Task finished
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

fn run_run_command(plan_path: &Path, machine_path: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("run command should execute");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn run_advances_linear_chain_to_completion() {
    let plan = r#"# Rhei: Linear Chain

## Tasks

### Task 1: First
**State:** pending

### Task 2: Second
**State:** pending
**Prior:** Task 1

### Task 3: Third
**State:** pending
**Prior:** Task 2
"#;

    let dir = unique_temp_dir("run-linear");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // All tasks should reach completed state (pending→in-progress→completed for each).
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    for task in &rhei.tasks {
        assert_eq!(
            task.state.as_str(),
            "completed",
            "Task {} should be completed, got {:?}",
            task.id,
            task.state
        );
    }

    // Should report 6 transitions (2 per task × 3 tasks).
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "should report 6 transitions; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("3/3 tasks in terminal state"),
        "should report all tasks terminal; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_advances_parallel_ready_tasks() {
    let plan = r#"# Rhei: Parallel Tasks

## Tasks

### Task 1: Root
**State:** completed

### Task 2: Branch A
**State:** pending
**Prior:** Task 1

### Task 3: Branch B
**State:** pending
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("run-parallel");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Both branches should complete.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    for task in &rhei.tasks {
        assert_eq!(task.state.as_str(), "completed", "Task {} should be completed", task.id);
    }

    // 4 transitions: 2 each for Task 2 and Task 3 (Task 1 already completed).
    assert!(
        result.stdout.contains("4 transition(s) made"),
        "should report 4 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_dry_run_shows_transitions_without_changes() {
    let plan = r#"# Rhei: Dry Run Test

## Tasks

### Task 1: Alpha
**State:** pending

### Task 2: Beta
**State:** pending
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("run-dry");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &["--dry-run"]);

    assert!(
        result.status.success(),
        "dry run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Should indicate what would happen.
    assert!(
        result.stdout.contains("Would transition Task 1"),
        "should show what would be transitioned; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("no changes were made"),
        "should indicate no changes; got:\n{}",
        result.stdout
    );

    // File should be unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan, "dry run should not modify the file");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_callback_failure_halts_execution() {
    let machine = r#"name: run-callback-test
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "cli:exit 1"
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: Callback Failure

## Tasks

### Task 1: Blocked
**State:** pending
"#;

    let dir = unique_temp_dir("run-callback-fail");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed (reports warning, no crash)\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Task should remain in pending since on_leave rejected.
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::Number(1)).expect("Task 1");
    assert_eq!(task.state.as_str(), "pending", "task should remain pending after callback failure");

    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "should report no progress; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_skips_already_completed_tasks() {
    let plan = r#"# Rhei: Already Done

## Tasks

### Task 1: Done
**State:** completed

### Task 2: Also done
**State:** completed
**Prior:** Task 1
"#;

    let dir = unique_temp_dir("run-already-done");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", RUN_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // No transitions should be made.
    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "should report nothing to advance; got:\n{}",
        result.stdout
    );

    // File should be unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan, "file should not be modified");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_no_callbacks_flag_skips_callbacks() {
    let machine = r#"name: run-nocb-test
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "cli:exit 1"
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: No Callbacks Run

## Tasks

### Task 1: Should advance
**State:** pending
"#;

    let dir = unique_temp_dir("run-no-callbacks");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run --no-callbacks should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Task should reach completed despite the failing callback.
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::Number(1)).expect("Task 1");
    assert_eq!(task.state.as_str(), "completed", "task should be completed with --no-callbacks");

    fs::remove_dir_all(dir).expect("cleanup");
}

// ── Directory Workspace tests ────────────────────────────────────────────────

const WORKSPACE_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

/// Helper: create a directory workspace with the given index content and
/// a set of task files. Returns the workspace root directory.
fn create_workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
    state_machine: &str,
) -> (PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        fs::write(tasks_dir.join(name), content).expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", state_machine);
    (ws, machine_path)
}

#[test]
fn workspace_loads_and_validates_correctly() {
    let (ws, machine_path) = create_workspace(
        "ws-valid",
        "# Rhei: Workspace Test\n\n## Context\nSome context here.\n",
        &[
            ("alpha.md", "### Task 1: Alpha\n**State:** pending\n\nAlpha description.\n"),
            (
                "beta.md",
                "### Task 2: Beta\n**State:** completed\n**Prior:** Task 1\n\nBeta description.\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // is_workspace detection
    assert!(workspace::is_workspace(&ws));

    // load_workspace produces merged plan
    let loaded = workspace::load_workspace(&ws).expect("load workspace");
    assert_eq!(loaded.rhei.title, "Workspace Test");
    assert_eq!(loaded.rhei.tasks.len(), 2);
    assert_eq!(loaded.task_sources.len(), 2);
    assert!(loaded.task_sources.contains_key("1"));
    assert!(loaded.task_sources.contains_key("2"));

    // CLI validate succeeds
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_render_json_includes_all_tasks() {
    let (ws, machine_path) = create_workspace(
        "ws-render",
        "# Rhei: Render Test\n",
        &[
            ("a.md", "### Task 1: First\n**State:** pending\n"),
            ("b.md", "### Task 2: Second\n**State:** completed\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("render")
        .arg(&ws)
        .arg("--format")
        .arg("json")
        .arg("--pretty")
        .output()
        .expect("render command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "render should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("\"title\": \"Render Test\""));
    assert!(stdout.contains("\"First\""));
    assert!(stdout.contains("\"Second\""));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_duplicate_task_id_across_files_is_reported() {
    let (ws, _machine_path) = create_workspace(
        "ws-dup",
        "# Rhei: Dup Test\n",
        &[
            ("a.md", "### Task 1: First\n**State:** pending\n"),
            ("b.md", "### Task 1: Duplicate\n**State:** pending\n"),
        ],
        fixtures::TEST_STATE_MACHINE,
    );

    let err = workspace::load_workspace(&ws).expect_err("should fail on duplicate");
    assert!(
        err.message.contains("duplicate task ID '1'"),
        "error should mention duplicate: {}",
        err.message
    );

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_missing_index_is_not_detected_as_workspace() {
    let dir = unique_temp_dir("ws-no-index");
    let ws = dir.join("workspace");
    fs::create_dir_all(ws.join("tasks")).expect("create dirs");

    assert!(!workspace::is_workspace(&ws));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn workspace_empty_tasks_directory_is_reported() {
    let (ws, _machine_path) =
        create_workspace("ws-empty", "# Rhei: Empty Test\n", &[], fixtures::TEST_STATE_MACHINE);

    let err = workspace::load_workspace(&ws).expect_err("should fail on empty");
    assert!(err.message.contains("no tasks"), "error should mention no tasks: {}", err.message);

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_transition_updates_correct_task_file() {
    let (ws, machine_path) = create_workspace(
        "ws-transition",
        "# Rhei: Transition Test\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("b.md", "### Task 2: Beta\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("transition")
        .arg(&ws)
        .arg("--task")
        .arg("1")
        .arg("--from")
        .arg("pending")
        .arg("--to")
        .arg("in-progress")
        .arg("--no-callbacks")
        .output()
        .expect("transition command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "transition should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify Task 1's file was updated.
    let a_content = fs::read_to_string(ws.join("tasks/a.md")).expect("read a.md");
    assert!(
        a_content.contains("**State:** in-progress"),
        "a.md should have updated state: {}",
        a_content
    );

    // Verify Task 2's file was NOT modified.
    let b_content = fs::read_to_string(ws.join("tasks/b.md")).expect("read b.md");
    assert!(b_content.contains("**State:** pending"), "b.md should be untouched: {}", b_content);

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_run_advances_tasks_to_completion() {
    let (ws, machine_path) = create_workspace(
        "ws-run",
        "# Rhei: Run Test\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("b.md", "### Task 2: Beta\n**State:** pending\n**Prior:** Task 1\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&ws)
        .arg("--no-callbacks")
        .output()
        .expect("run command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "run should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );

    // Both tasks should reach completed.
    let loaded = workspace::load_workspace(&ws).expect("reload workspace");
    for task in &loaded.rhei.tasks {
        assert_eq!(task.state.as_str(), "completed", "Task {} should be completed", task.id);
    }

    assert!(stdout.contains("Run complete"));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_index_with_tasks_section_is_rejected() {
    let index = "# Rhei: Bad Index\n\n## Tasks\n\n### Task 1: Oops\n**State:** pending\n";
    let err = rhei_core::parser::parse_workspace_index(index)
        .expect_err("should reject Tasks section in index");
    assert!(
        err.message.contains("must not contain a '## Tasks' section"),
        "error: {}",
        err.message
    );
}
