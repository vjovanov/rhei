use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rhei_core::ast::TaskId;
use rhei_core::parse;
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

const CLI_VALID_PLAN: &str = r#"# Saga: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Subtask 1.1: Capture deployment events
List all event types emitted by the deployment system.

#### Subtask 1.2: Record rollback contract
```yaml
rollback:
  enabled: true
```

### Task bootstrap_env: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Subtask 2.1: Provision staging secrets
Create and store staging credentials.

### Task 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task bootstrap_env

#### Subtask 3.1: Dry run in staging
Run the bot in dry-run mode against staging.
"#;

const CLI_PRIMARY_ERROR_REGRESSION_PLAN: &str = r#"# Saga: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Subtask 1.1: Capture deployment events
List all event types emitted by the deployment system.

### Task bootstrap_env: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Subtask 2.1: Provision staging secrets
Create and store staging credentials.

### Tak 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task bootstrap_env

#### Subtask 3.1: Dry run in staging
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
    let saga = parse(CLI_VALID_PLAN).expect("valid fixture should parse");
    let machine =
        StateMachine::from_yaml_str(fixtures::TEST_STATE_MACHINE).expect("machine should load");
    let report = validate_with_machine(&saga, &machine);

    assert!(
        !report.has_errors(),
        "expected valid plan fixture to pass validation, got: {:?}",
        report.errors
    );
    assert_eq!(report.warnings.len(), 1, "expected named-task numbering warning");
    assert!(report.warnings[0]
        .contains("Cannot validate subtask numbering for named task 'bootstrap_env'"));

    assert_eq!(saga.title, "Release Automation Rollout");
    assert_eq!(saga.tasks.len(), 3);
    assert_eq!(saga.tasks[0].id, TaskId::Number(1));
    assert_eq!(saga.tasks[1].id, TaskId::Named("bootstrap_env".to_string()));
    assert_eq!(saga.tasks[2].metadata.depends_on.len(), 2);

    let json = to_json_value(&saga);
    assert_eq!(json["title"].as_str(), Some("Release Automation Rollout"));
    assert_eq!(json["tasks"].as_array().map(Vec::len), Some(3));

    let github = to_github_markdown(&saga);
    assert!(github.contains("### Task 1: Define pipeline contracts"));
    assert!(github.contains("### Task bootstrap_env: Bootstrap environments"));
    assert!(github.contains("- Prior: Task 1, Task bootstrap_env"));
    assert!(github.contains("- [ ] 3.1: Dry run in staging"));

    let progress = ProgressReportOutput { color: false, show_dependencies: true }.to_string(&saga);
    assert!(progress.contains("Saga: Release Automation Rollout"));
    assert!(progress.contains("* Task bootstrap_env: Bootstrap environments  [IN-PROGRESS]"));
    assert!(progress.contains("  - Prior: Task 1, Task bootstrap_env"));
}

#[test]
fn invalid_plan_reports_cross_component_validation_failures() {
    let saga = parse(fixtures::INVALID_PLAN).expect("invalid semantic fixture should still parse");
    let machine =
        StateMachine::from_yaml_str(fixtures::TEST_STATE_MACHINE).expect("machine should load");
    let report = validate_with_machine(&saga, &machine);

    assert!(report.has_errors(), "expected semantic validation errors");
    let joined = report.errors.join("\n");

    assert!(joined.contains("Task 1 metadata order invalid"));
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
    assert!(render_stdout.contains("\"named\": \"bootstrap_env\""));

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
    assert!(stderr.contains("Task 1 metadata order invalid"));
    assert!(stderr.contains("Circular dependency detected"));

    fs::remove_dir_all(temp_dir).expect("temporary directory should be removed");
}

#[test]
fn cli_validate_reports_missing_saga_header_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MISSING_SAGA_HEADER,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-missing-saga",
    );

    assert_parse_failure(
        &result,
        &["Missing", "Saga:", "header"],
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
fn cli_validate_reports_malformed_saga_header_parse_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_SAGA_HEADER,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-saga",
    );

    assert_parse_failure(
        &result,
        &["Malformed saga heading", "expected", "Saga:", "title"],
        Some("line 1"),
        Some("#Saga: Missing required space"),
        &["VALIDATION ERROR", "missing mandatory **State:**"],
    );
}

#[test]
fn cli_validate_reports_h1_typo_as_malformed_saga_header_parse_failure() {
    let result = run_validate(
        r#"# Sga: Release Automation Rollout

## Tasks

### Task 1: Primary task
**State:** pending
"#,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-saga-heading-typo",
    );

    assert_parse_failure(
        &result,
        &["Malformed saga heading", "expected", "Saga:", "title"],
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
        &["missing mandatory **State:**", "metadata order invalid", "Circular dependency detected"],
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
        &["depends on missing Task", "metadata order invalid"],
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
fn cli_validate_reports_missing_state_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MISSING_STATE,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-missing-state",
    );

    assert_validation_failure(
        &result,
        &[&["Task 1", "missing mandatory", "State:", "metadata"]],
        &["failed to parse", "Malformed metadata field"],
    );
}

#[test]
fn cli_validate_reports_prior_before_state_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_PRIOR_BEFORE_STATE,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-prior-before-state",
    );

    assert_validation_failure(
        &result,
        &[&["Task 1", "metadata order invalid", "State:", "must appear before", "Prior:"]],
        &["failed to parse", "Malformed metadata field"],
    );
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
fn cli_validate_surfaces_named_task_warning_on_success() {
    let result = run_validate(
        CLI_VALID_PLAN,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-warning-success",
    );

    assert!(
        result.status.success(),
        "expected successful validation with warning\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("Validation succeeded"));
    assert!(
        result
            .stdout
            .contains("warning: Cannot validate subtask numbering for named task 'bootstrap_env'"),
        "expected named-task warning in stdout, got:\n{}",
        result.stdout
    );
    assert!(
        !result.stderr.contains("PARSE ERROR") && !result.stderr.contains("VALIDATION ERROR"),
        "successful warning case should not emit failure diagnostics, got:\n{}",
        result.stderr
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
        Some("line 18"),
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
