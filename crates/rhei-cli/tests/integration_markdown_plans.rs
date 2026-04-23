use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rhei_core::ast::TaskId;
use rhei_core::parse;
use rhei_core::parser::parse_workspace_index;
use rhei_core::workspace;
use rhei_output::{to_github_markdown, to_json_value, ProgressReportOutput};
use rhei_validator::{validate_with_machine, StateMachine};
use serde_yaml::Value as YamlValue;

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

fn yaml_key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

fn visit_count_from_metadata(
    metadata: Option<&rhei_core::ast::Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<u64> {
    let metadata = metadata?;
    let metadata_section = metadata.get(yaml_key("metadata"))?.as_mapping()?;
    let tasks = metadata_section.get(yaml_key("tasks"))?.as_mapping()?;
    let task_key = if let Some(n) = task_id.as_number() {
        serde_yaml::to_value(n).ok()?
    } else if let Some(name) = task_id.as_named() {
        yaml_key(name)
    } else {
        // Dotted ids are serialized as their dotted string form.
        yaml_key(&task_id.to_string())
    };
    let task = tasks.get(task_key)?.as_mapping()?;
    let state_visits = task.get(yaml_key("stateVisits"))?.as_mapping()?;
    state_visits.get(yaml_key(state_name))?.as_u64()
}

const CLI_VALID_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Task 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

#### Task 1.2: Record rollback contract
**State:** completed
```yaml
rollback:
  enabled: true
```

### Task 2: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Task 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Task 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task 2

#### Task 3.1: Dry run in staging
**State:** pending
Run the bot in dry-run mode against staging.
"#;

// The first parse error the parser should surface is the malformed `### Tak 3:`
// heading at line 20 (unknown node kind). Earlier tasks are intentionally
// well-formed so this regression test can confirm that the malformed top-level
// heading is reported before any later child-id extension concerns.
const CLI_PRIMARY_ERROR_REGRESSION_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

#### Task 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

### Task 2: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

#### Task 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Tak 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task 2

#### Task 3.1: Dry run in staging
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

fn run_cli_without_args() -> CliRun {
    let output =
        Command::new(env!("CARGO_BIN_EXE_rhei")).output().expect("rhei command should run");

    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
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

#[test]
fn bare_cli_prints_help_and_exits_successfully() {
    let result = run_cli_without_args();

    assert!(
        result.status.success(),
        "bare CLI invocation should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.trim().is_empty(),
        "help output should not be written to stderr:\n{}",
        result.stderr
    );
    assert!(
        result.stdout.contains("Usage: rhei [OPTIONS] <COMMAND>"),
        "help output should include usage:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Validate and compile markdown plans into structured outputs"),
        "help output should include the CLI summary:\n{}",
        result.stdout
    );
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
    assert_eq!(rhei.tasks[0].id, TaskId::number(1));
    assert_eq!(rhei.tasks[1].id, TaskId::number(2));
    assert_eq!(rhei.tasks[2].prior.len(), 2);

    let json = to_json_value(&rhei);
    assert_eq!(json["title"].as_str(), Some("Release Automation Rollout"));
    assert_eq!(json["tasks"].as_array().map(Vec::len), Some(3));

    let github = to_github_markdown(&rhei);
    assert!(github.contains("### Task 1: Define pipeline contracts"));
    assert!(github.contains("### Task 2: Bootstrap environments"));
    assert!(github.contains("- Prior: Task 1, Task 2"));
    assert!(github.contains("#### Task 3.1: Dry run in staging"));

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

    // The "subtask under wrong parent" assertion was removed: the rule that a
    // child id must extend its parent's id is now enforced by the parser, not
    // the validator, so a mismatched child would fail at parse time instead of
    // surfacing here. The circular-dependency check still belongs here.
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
        &["unknown node kind", "Tak"],
        Some("line 5"),
        Some("### Tak 1: Broken keyword"),
        &["Task 1.1", "missing mandatory **State:**"],
    );
}

#[test]
fn cli_validate_reports_child_id_not_extending_parent_as_parse_failure() {
    // Renamed from cli_validate_reports_malformed_subtask_heading_parse_failure.
    // The "Missing decimal component" fixture now parses as `#### Task 1: ...`,
    // but since `1` does not extend parent id `1` by exactly one segment, the
    // parser rejects it with the new id-extension error.
    let result = run_validate(
        fixtures::INVALID_FIXTURE_MALFORMED_SUBTASK_HEADING,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-child-id-extension",
    );

    assert_parse_failure(
        &result,
        &["heading depth", "does not match id path depth"],
        Some("line 8"),
        Some("#### Task 1: Missing decimal component"),
        &["missing mandatory **State:**"],
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
fn cli_validate_reports_child_id_parent_mismatch_as_parse_failure() {
    // Renamed from cli_validate_reports_subtask_parent_mismatch_as_semantic_failure.
    // A mismatched child id is now a parse-time error, not a validator-level
    // error, because the rule "child id must extend parent id by exactly one
    // segment" moved into the parser.
    let result = run_validate(
        fixtures::INVALID_FIXTURE_SUBTASK_PARENT_MISMATCH,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-child-id-mismatch",
    );

    assert_parse_failure(
        &result,
        &["child id", "must extend parent id", "exactly one segment"],
        None,
        Some("#### Task 1.1: Wrong parent prefix"),
        &["VALIDATION ERROR"],
    );
}

// Removed: `cli_validate_rejects_named_task_with_subtasks`. The rule that
// named tasks cannot have subtasks has been removed — any task, whether its
// id is numeric or named, may have children as long as each child id extends
// its parent by exactly one segment. The removed test's input (a named task
// `build` with a child `1.1`) is now a parse error (1.1 does not extend
// `build`) rather than a validator error.

#[test]
fn malformed_task_heading_reports_parse_error_instead_of_child_id_validation_error() {
    let result = run_validate(
        CLI_PRIMARY_ERROR_REGRESSION_PLAN,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-malformed-heading",
    );

    assert_parse_failure(
        &result,
        &["unknown node kind", "Tak"],
        Some("line 20"),
        Some("### Tak 3: Roll out release bot"),
        &["child id", "must extend parent id"],
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

const COUNTED_LOOP_STATE_MACHINE: &str = r#"name: counted-loop
version: 1
states:
  pending:
    description: ready
    initial: true
  agent-review:
    description: review
    visits: 2
  agent-review-fix:
    description: fix
  human-review:
    description: human gate
  completed:
    description: done
    final: true
transitions:
  - from: pending
    to: agent-review
  - from: agent-review
    to: agent-review-fix
    condition: visitCount < visits
  - from: agent-review
    to: human-review
    condition: visitCount >= visits
  - from: agent-review-fix
    to: agent-review
  - from: human-review
    to: completed
"#;

const COUNTED_LOOP_PLAN: &str = r#"# Rhei: Counted Review Loop

## Tasks

### Task 1: Review me
**State:** pending
"#;

const COMPLETE_STATE_MACHINE: &str = r#"name: complete-test-machine
version: 1
states:
  pending:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
  cancelled:
    description: Task cancelled
    final: true
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
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

fn run_complete(plan_path: &Path, machine_path: &Path, task: &str, result_msg: &str) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(machine_path)
        .arg("complete")
        .arg(plan_path)
        .arg("--task")
        .arg(task)
        .arg("--result")
        .arg(result_msg)
        .arg("--no-callbacks")
        .output()
        .expect("complete command should run");

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
    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task1.state.as_str(), "in-progress");

    // Task 2 should be untouched.
    let task2 = rhei.tasks.iter().find(|t| t.id == TaskId::number(2)).expect("Task 2 exists");
    assert_eq!(task2.state.as_str(), "in-progress");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_counted_loop_updates_metadata_and_blocks_exhausted_reentry() {
    let dir = unique_temp_dir("transition-counted-loop");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", COUNTED_LOOP_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let first = run_transition(&plan_path, &machine_path, "1", "pending", "agent-review");
    assert!(first.status.success(), "initial review transition should succeed: {}", first.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after first transition");
    let rhei = parse(&updated).expect("parse updated plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(1)
    );

    let fail_then_fix =
        run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(
        fail_then_fix.status.success(),
        "review -> fix should succeed: {}",
        fail_then_fix.stderr
    );

    let reenter =
        run_transition(&plan_path, &machine_path, "1", "agent-review-fix", "agent-review");
    assert!(reenter.status.success(), "fix -> review should succeed: {}", reenter.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after re-entry");
    let rhei = parse(&updated).expect("parse re-entered plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(2)
    );

    let exhausted =
        run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(!exhausted.status.success(), "exhausted review loop should reject re-entry");
    assert!(
        normalize_for_assertions(&exhausted.stderr).contains("evaluated to false"),
        "expected loop-budget rejection, got:\n{}",
        exhausted.stderr
    );

    let escalate = run_transition(&plan_path, &machine_path, "1", "agent-review", "human-review");
    assert!(
        escalate.status.success(),
        "human review escalation should succeed: {}",
        escalate.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn transition_from_authored_counted_state_treats_start_as_first_visit() {
    let dir = unique_temp_dir("transition-authored-counted-loop");
    let plan = r#"# Rhei: Authored Counted Review Loop

## Tasks

### Task 1: Start in review
**State:** agent-review
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let to_fix = run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(to_fix.status.success(), "review -> fix should succeed: {}", to_fix.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after leaving authored review");
    let rhei = parse(&updated).expect("parse updated plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(1)
    );

    let reenter =
        run_transition(&plan_path, &machine_path, "1", "agent-review-fix", "agent-review");
    assert!(reenter.status.success(), "fix -> review should succeed: {}", reenter.stderr);

    let updated = fs::read_to_string(&plan_path).expect("read plan after re-entering review");
    assert!(
        updated.contains("**State:** agent-review-2"),
        "expected visible counted visit suffix after re-entry:\n{}",
        updated
    );
    let rhei = parse(&updated).expect("parse re-entered plan");
    assert_eq!(
        visit_count_from_metadata(rhei.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(2)
    );

    let exhausted =
        run_transition(&plan_path, &machine_path, "1", "agent-review", "agent-review-fix");
    assert!(!exhausted.status.success(), "second re-entry should exhaust the visit budget");
    assert!(
        normalize_for_assertions(&exhausted.stderr).contains("evaluated to false"),
        "expected loop-budget rejection, got:\n{}",
        exhausted.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_accepts_counted_state_suffix_within_budget() {
    let input = r#"# Rhei: Counted State Suffix
## Tasks

### Task 1: Review
**State:** agent-review-2
"#;

    let rhei = parse(input).expect("parse ok");
    let report = validate_with_machine(
        &rhei,
        &rhei_validator::StateMachine::from_yaml_str(COUNTED_LOOP_STATE_MACHINE)
            .expect("state machine"),
    );

    assert!(
        !report.has_errors(),
        "counted state suffix within budget should validate: {:?}",
        report.errors
    );
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
    let task =
        rhei.tasks.iter().find(|t| t.id == TaskId::named("setup")).expect("Task setup exists");
    assert_eq!(task.state.as_str(), "in-progress");

    // Task build should be untouched.
    let build =
        rhei.tasks.iter().find(|t| t.id == TaskId::named("build")).expect("Task build exists");
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
    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task1.state.as_str(), "cancelled");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_rejects_parent_with_non_terminal_subtasks() {
    let plan = r#"# Rhei: Parent Completion Guard

## Tasks

### Task 1: Parent task
**State:** pending

#### Task 1.1: Open item
**State:** pending
"#;

    let dir = unique_temp_dir("complete-open-subtasks");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    let result = run_complete(&plan_path, &machine_path, "1", "done");

    assert!(!result.status.success(), "complete should fail when children are non-terminal");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("cannot be completed while child tasks remain non-terminal"),
        "expected child-task guard in stderr, got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("Task 1.1"),
        "expected offending child task id in stderr, got:\n{}",
        result.stderr
    );
    assert!(
        normalized.contains("('Open item') [pending]"),
        "expected offending child task state in stderr, got:\n{}",
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "pending");
    assert_eq!(task.children[0].state.as_str(), "pending");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn complete_succeeds_when_all_subtasks_are_terminal() {
    let plan = r#"# Rhei: Parent Completion Success

## Tasks

### Task 1: Parent task
**State:** pending

#### Task 1.1: Closed item
**State:** completed
"#;

    let dir = unique_temp_dir("complete-terminal-subtasks");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COMPLETE_STATE_MACHINE);

    let result = run_complete(&plan_path, &machine_path, "1", "done");

    assert!(
        result.status.success(),
        "complete should succeed when subtasks are terminal\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "completed");
    assert_eq!(task.children[0].state.as_str(), "completed");
    assert!(
        updated.contains("> **Result:** [1](runtime/results/1.md)"),
        "expected result link in updated plan:\n{}",
        updated
    );

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
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
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
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
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

#[test]
fn callback_rejection_surfaces_spec_error_message() {
    // A callback returns `{"success": false, "error": "..."}` per the spec;
    // the CLI should surface the message verbatim and leave the plan unchanged.
    let machine_yaml = r#"name: spec-rejection
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
    on_leave: 'cli:printf ''{"success": false, "error": "dep not met"}'''
  - from: in-progress
    to: completed
"#;
    let dir = unique_temp_dir("callback-spec-rejection");
    let plan = r#"# Rhei: Spec Rejection Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(!result.status.success(), "spec-style rejection should fail the transition");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("dep not met"),
        "stderr should carry the callback's error message; got:\n{}",
        result.stderr
    );

    // File unchanged.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_redirect_via_next_state_retargets_declared_transition() {
    // `on_leave` returns a `nextState` that targets a different declared
    // transition from the same `from`; the CLI should follow the redirect.
    let machine_yaml = r#"name: spec-redirect
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  rejected:
    description: Rejected outright
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: 'cli:printf ''{"success": true, "nextState": "rejected"}'''
  - from: pending
    to: rejected
"#;
    let dir = unique_temp_dir("callback-spec-redirect");
    let plan = r#"# Rhei: Spec Redirect Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        result.status.success(),
        "redirect to a declared target should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Task should end up in the redirected target, not the originally-requested one.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "rejected");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_redirect_to_undeclared_transition_is_rejected() {
    let machine_yaml = r#"name: spec-redirect-undeclared
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  elsewhere:
    description: A state with no transition declared from `pending`.
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: 'cli:printf ''{"success": true, "nextState": "elsewhere"}'''
"#;
    let dir = unique_temp_dir("callback-spec-redirect-bad");
    let plan = r#"# Rhei: Spec Redirect Bad Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        !result.status.success(),
        "redirect to an undeclared transition should fail the transition"
    );
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("elsewhere") && normalized.contains("no transition"),
        "stderr should explain the undeclared redirect; got:\n{}",
        result.stderr
    );

    // File unchanged — redirect was rejected before any write.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_receives_transition_context_on_stdin() {
    // The callback reads its stdin and writes it back into a file we
    // then inspect. This verifies the TransitionContext JSON payload is
    // actually delivered on stdin.
    let dir = unique_temp_dir("callback-spec-stdin");
    let capture_path = dir.join("captured.json");
    let capture_display = capture_path.display().to_string();

    let machine_yaml = format!(
        r#"name: spec-stdin
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
    on_leave: "cli:cat > '{capture}'"
"#,
        capture = capture_display
    );

    let plan = r#"# Rhei: Spec Stdin Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);
    assert!(
        result.status.success(),
        "transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let captured =
        fs::read_to_string(&capture_path).expect("callback should have written stdin payload");
    let parsed: serde_json::Value =
        serde_json::from_str(&captured).expect("captured payload should be JSON");
    assert_eq!(parsed["task"]["id"], "1");
    assert_eq!(parsed["task"]["title"], "Alpha");
    assert_eq!(parsed["transition"]["from"], "pending");
    assert_eq!(parsed["transition"]["to"], "in-progress");
    assert_eq!(parsed["transition"]["triggeredBy"], "user");
    assert_eq!(parsed["environment"]["platform"], "cli");
    assert!(parsed["transition"]["timestamp"].is_string());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_on_enter_failure_rolls_back_state_write() {
    // The on_leave callback approves; the on_enter callback crashes.
    // Per the spec, the state write must roll back to its pre-transition
    // contents rather than persisting the mid-transition state.
    let machine_yaml = r#"name: spec-on-enter-rollback
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
    on_enter: "cli:exit 1"
"#;
    let dir = unique_temp_dir("callback-spec-on-enter-rollback");
    let plan = r#"# Rhei: On-Enter Rollback Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(!result.status.success(), "on_enter failure should fail the transition");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("on_enter") && normalized.contains("failed"),
        "stderr should identify the on_enter failure; got:\n{}",
        result.stderr
    );

    // Most importantly: the plan file must be rolled back to its original state.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan, "on_enter failure must roll back the state write");

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

fn run_run_command_in_dir(
    current_dir: &Path,
    plan_path: &Path,
    machine_path: &Path,
    extra_args: &[&str],
) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.current_dir(current_dir).arg("--state-machine").arg(machine_path).arg("run").arg(plan_path);
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

fn run_reset_command(plan_path: &Path, machine_path: &Path) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(machine_path)
        .arg("reset")
        .arg(plan_path)
        .output()
        .expect("reset command should run");

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
fn run_uses_counted_loop_exit_when_visit_budget_is_exhausted() {
    let plan = r#"# Rhei: Exhausted Loop

---
metadata:
  tasks:
    1:
      stateVisits:
        agent-review: 2
---

## Tasks

### Task 1: Needs escalation
**State:** agent-review
"#;

    let dir = unique_temp_dir("run-counted-loop");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task exists");
    assert_eq!(task.state, "completed");
    assert!(
        !result.stdout.contains("agent-review-fix"),
        "run should escalate instead of looping through fix once exhausted; got:\n{}",
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
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1");
    assert_eq!(task.state.as_str(), "pending", "task should remain pending after callback failure");

    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "should report no progress; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_executes_relative_callback_from_state_machine_directory() {
    let dir = unique_temp_dir("run-relative-callback");
    let workspace_dir = dir.join("examples");
    let machine_dir = workspace_dir.join("bash-agent-team");
    fs::create_dir_all(&machine_dir).expect("create machine dir");

    let plan = r#"# Rhei: Relative Callback

## Tasks

### Task 1: Bootstrap
**State:** pending
"#;
    let machine = r#"name: relative-callback
version: 1
states:
  pending:
    initial: true
  completed:
    final: true
transitions:
  - from: pending
    to: completed
    on_leave: "cli:bash ./workflow.sh"
"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$(dirname "$RHEI_PLAN_PATH")/runtime"
printf '%s\n' "$RHEI_PLAN_PATH" > "$(dirname "$RHEI_PLAN_PATH")/runtime/plan-path.txt"
"#;

    let plan_path = write_fixture_file(&workspace_dir, "release-automation.rhei.md", plan);
    write_fixture_file(&machine_dir, "team-states.yaml", machine);
    let script_path = write_fixture_file(&machine_dir, "workflow.sh", script);
    let mut perms = fs::metadata(&script_path).expect("stat workflow").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod workflow");
    }

    let result = run_run_command_in_dir(
        &workspace_dir,
        Path::new("release-automation.rhei.md"),
        Path::new("bash-agent-team/team-states.yaml"),
        &[],
    );

    assert!(
        result.status.success(),
        "run should succeed with callbacks relative to the state machine path\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Task 1 transitioned: 'pending' → 'completed'"),
        "expected transition output; got:\n{}",
        result.stdout
    );

    let recorded_plan_path = fs::read_to_string(workspace_dir.join("runtime/plan-path.txt"))
        .expect("read callback output");
    assert_eq!(
        Path::new(recorded_plan_path.trim()),
        plan_path.canonicalize().expect("canonicalize plan path"),
        "callbacks should receive an absolute plan path",
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_executes_all_models_callbacks_without_agent_configuration() {
    let dir = unique_temp_dir("run-all-models-callback");
    let plan = r#"# Rhei: Multi-Model Callback

## Tasks

### Task review-seed: Review specs
**State:** review
"#;
    let machine = r#"name: multi-model-callback
version: 1
models:
  - claude
  - codex
states:
  review:
    initial: true
    all_models: [claude, codex]
    outputs:
      - name: findings
        path: runtime/{model}-findings.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
    on_leave: "cli:bash ./workflow.sh"
"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
: "${RHEI_MODEL:?RHEI_MODEL must be set}"
runtime_dir="$(dirname "$RHEI_PLAN_PATH")/runtime"
mkdir -p "$runtime_dir"
printf '%s\n' "$RHEI_MODEL" >> "$runtime_dir/models.txt"
printf '# Findings for %s\n' "$RHEI_MODEL" > "$runtime_dir/$RHEI_MODEL-findings.md"
"#;

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "workflow.sh", script);
    let mut perms = fs::metadata(&script_path).expect("stat workflow").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod workflow");
    }

    let result = run_run_command(&plan_path, &machine_path, &[]);

    assert!(
        result.status.success(),
        "run should succeed for callback-only all_models state\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei
        .tasks
        .iter()
        .find(|task| task.id == TaskId::named("review-seed"))
        .expect("review-seed exists");
    assert_eq!(task.state.as_str(), "completed");

    let models = fs::read_to_string(dir.join("runtime/models.txt")).expect("read model log");
    assert_eq!(models, "claude\ncodex\n");
    assert!(dir.join("runtime/claude-findings.md").exists(), "claude artifact should exist");
    assert!(dir.join("runtime/codex-findings.md").exists(), "codex artifact should exist");

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
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1");
    assert_eq!(task.state.as_str(), "completed", "task should be completed with --no-callbacks");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn reset_restores_single_file_plan_to_initial_state() {
    let machine = r#"name: reset-test
version: 1
states:
  draft:
    description: Start here
    initial: true
  pending:
    description: Ready
  in-progress:
    description: Active
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: pending
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

    let plan = r#"# Rhei: Resettable

## Tasks

### Task 1: Alpha
**State:** completed

#### Task 1.1: Detail
**State:** in-progress

### Task 2: Beta
**State:** pending
"#;

    let dir = unique_temp_dir("reset-single-file");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_reset_command(&plan_path, &machine_path);

    assert!(
        result.status.success(),
        "reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result
            .stdout
            .contains("Reset 2 task(s) (and 1 descendant task(s)) to initial state 'draft'."),
        "unexpected stdout:\n{}",
        result.stdout
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse reset plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "draft");
    assert_eq!(rhei.tasks[0].children[0].state.as_str(), "draft");
    assert_eq!(rhei.tasks[1].state.as_str(), "draft");

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
fn validate_auto_discovers_workspace_root_state_machine_from_states_declaration() {
    let dir = unique_temp_dir("ws-auto-states");
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(
        ws.join("index.rhei.md"),
        "# Rhei: Workspace Auto States\n**States:** workspace-test-machine\n",
    )
    .expect("write index");
    fs::write(tasks_dir.join("alpha.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task file");
    write_fixture_file(&ws, "states.yaml", WORKSPACE_STATE_MACHINE);

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
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

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_reports_mismatched_auto_discovered_state_machine_name() {
    let dir = unique_temp_dir("auto-states-mismatch");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Auto States Mismatch\n**States:** custom-review\n\n## Tasks\n\n### Task 1: Review docs\n**State:** draft\n",
    );
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: wrong-machine\nversion: 1\nstates:\n  draft:\n    initial: true\n    description: Start\n  completed:\n    final: true\n    description: Done\ntransitions:\n  - from: draft\n    to: completed\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "validate should fail when auto-discovered machine name mismatches\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(
        stderr.contains("plan declares state machine 'custom-review'"),
        "expected mismatch diagnostic, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("declares 'wrong-machine'"),
        "expected discovered machine name in diagnostic, got:\n{}",
        stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
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
fn workspace_transition_updates_index_metadata_for_counted_loops() {
    let dir = unique_temp_dir("workspace-counted-loop");
    fs::create_dir_all(dir.join("tasks")).expect("create tasks dir");
    write_fixture_file(
        &dir,
        "index.rhei.md",
        r#"# Rhei: Workspace Counted Loop

## Overview
Metadata lives here.
"#,
    );
    write_fixture_file(
        &dir.join("tasks"),
        "01-review.md",
        r#"### Task 1: Review task
**State:** pending
"#,
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_LOOP_STATE_MACHINE);

    let result = run_transition(&dir, &machine_path, "1", "pending", "agent-review");
    assert!(
        result.status.success(),
        "workspace transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let index_raw = fs::read_to_string(dir.join("index.rhei.md")).expect("read index");
    let index = parse_workspace_index(&index_raw).expect("parse index");
    assert_eq!(
        visit_count_from_metadata(index.metadata.as_ref(), &TaskId::number(1), "agent-review"),
        Some(1)
    );

    let task_raw = fs::read_to_string(dir.join("tasks/01-review.md")).expect("read task file");
    let tasks = rhei_core::parser::parse_workspace_tasks(&task_raw).expect("parse task file");
    assert_eq!(tasks[0].state, "agent-review");

    fs::remove_dir_all(dir).expect("cleanup");
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
fn workspace_reset_restores_initial_states_and_removes_runtime() {
    let (ws, machine_path) = create_workspace(
        "ws-reset",
        "# Rhei: Reset Test\n",
        &[
            (
                "a.md",
                "### Task 1: Alpha\n**State:** completed\n\n#### Task 1.1: Detail\n**State:** in-progress\n",
            ),
            ("b.md", "### Task 2: Beta\n**State:** in-progress\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let runtime_dir = ws.join("runtime/logs");
    fs::create_dir_all(&runtime_dir).expect("create runtime dir");
    fs::write(runtime_dir.join("team.log"), "generated").expect("write runtime log");

    let result = run_reset_command(&ws, &machine_path);

    assert!(
        result.status.success(),
        "workspace reset should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result
            .stdout
            .contains("Reset 2 task(s) (and 1 descendant task(s)) to initial state 'pending'."),
        "unexpected stdout:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Removed runtime output."),
        "expected runtime cleanup message, got:\n{}",
        result.stdout
    );

    let loaded = workspace::load_workspace(&ws).expect("reload workspace");
    assert_eq!(loaded.rhei.tasks[0].state.as_str(), "pending");
    assert_eq!(loaded.rhei.tasks[0].children[0].state.as_str(), "pending");
    assert_eq!(loaded.rhei.tasks[1].state.as_str(), "pending");
    assert!(!ws.join("runtime").exists(), "runtime directory should be removed");

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn assignee_field_round_trips_through_parse_and_json() {
    let input = "# Rhei: Roundtrip\n\n\
## Tasks\n\n\
### Task 1: Alpha\n\
**State:** in-progress\n\
**Prior:** Task 2\n\
**Assignee:** alice\n\n\
### Task 2: Beta\n\
**State:** pending\n";

    let rhei = parse(input).expect("parse ok");

    let task1 = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("task 1");
    assert_eq!(task1.assignee.as_deref(), Some("alice"));
    let task2 = rhei.tasks.iter().find(|t| t.id == TaskId::number(2)).expect("task 2");
    assert_eq!(task2.assignee, None);

    let json = to_json_value(&rhei);
    let tasks = json["tasks"].as_array().expect("tasks array");
    let t1 = tasks.iter().find(|t| t["id"]["path"].as_str() == Some("1")).expect("task 1 json");
    assert_eq!(t1["assignee"].as_str(), Some("alice"));
    let t2 = tasks.iter().find(|t| t["id"]["path"].as_str() == Some("2")).expect("task 2 json");
    assert!(t2.as_object().unwrap().get("assignee").is_none());

    let md = to_github_markdown(&rhei);
    assert!(md.contains("- Assignee: alice"), "expected assignee in markdown output:\n{md}");
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
