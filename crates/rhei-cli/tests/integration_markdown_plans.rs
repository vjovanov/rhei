use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rhei_core::ast::TaskId;
use rhei_core::parse;
use rhei_output::{to_github_markdown, to_json_value, ProgressReportOutput};
use rhei_validator::{validate_with_machine, StateMachine};

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

#[test]
fn valid_plan_parses_validates_and_renders_across_crates() {
    let saga = parse(fixtures::VALID_PLAN).expect("valid fixture should parse");
    let machine =
        StateMachine::from_yaml_str(fixtures::TEST_STATE_MACHINE).expect("machine should load");
    let report = validate_with_machine(&saga, &machine);

    assert!(
        !report.has_errors(),
        "expected valid plan fixture to pass validation, got: {:?}",
        report.errors
    );
    assert_eq!(report.warnings.len(), 1, "expected named-task numbering warning");
    assert!(report.warnings[0].contains("Cannot validate subtask numbering for named task 'bootstrap_env'"));

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

    let progress = ProgressReportOutput {
        color: false,
        show_dependencies: true,
    }
    .to_string(&saga);
    assert!(progress.contains("Saga: Release Automation Rollout"));
    assert!(progress.contains(
        "* Task bootstrap_env: Bootstrap environments  [IN-PROGRESS]"
    ));
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
    assert!(joined.contains(
        "Subtask 2.1 ('Wrong subtask parent') is under Task 1 but declares parent 2"
    ));
    assert!(joined.contains("Circular dependency detected"));
}

#[test]
fn cli_validate_and_render_use_real_fixture_files() {
    let temp_dir = unique_temp_dir("integration-cli");
    let plan_path = write_fixture_file(&temp_dir, "valid-plan.md", fixtures::VALID_PLAN);
    let machine_path =
        write_fixture_file(&temp_dir, "states.yaml", fixtures::TEST_STATE_MACHINE);

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
    let machine_path =
        write_fixture_file(&temp_dir, "states.yaml", fixtures::TEST_STATE_MACHINE);

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
    assert!(stderr.contains("validation failed"));
    assert!(stderr.contains("Task 1 metadata order invalid"));
    assert!(stderr.contains("Circular dependency detected"));

    fs::remove_dir_all(temp_dir).expect("temporary directory should be removed");
}

#[test]
fn malformed_task_heading_reports_parse_error_instead_of_subtask_validation_error() {
    let temp_dir = unique_temp_dir("integration-cli-malformed-heading");
    let plan_path = write_fixture_file(
        &temp_dir,
        "malformed-plan.md",
        include_str!("../../../examples/release-automation.saga.md"),
    );
    let machine_path =
        write_fixture_file(&temp_dir, "states.yaml", fixtures::TEST_STATE_MACHINE);

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    assert!(
        !output.status.success(),
        "malformed plan should fail validation"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to parse"),
        "expected parse failure, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Malformed task heading"),
        "expected malformed-heading message, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("### Tak 3: Roll out release bot"),
        "expected diagnostic to include the malformed heading line, got:\n{}",
        stderr
    );
    assert!(
        !stderr.contains("Subtask 3.1 ('Dry run in staging') is under Task 2 but declares parent 3"),
        "should not surface misleading subtask-parent error, got:\n{}",
        stderr
    );

    fs::remove_dir_all(temp_dir).expect("temporary directory should be removed");
}
