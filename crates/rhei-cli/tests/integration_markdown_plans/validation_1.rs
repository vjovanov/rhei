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
