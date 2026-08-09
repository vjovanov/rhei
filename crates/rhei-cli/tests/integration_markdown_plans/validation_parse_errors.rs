
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
fn cli_validate_accumulates_single_file_parse_errors() {
    let result = run_validate(
        r#"# Rhei: Multiple Parse Errors

## Tasks

### Task 1: Missing state

### Task 2: Prior typo
**Prior** Task 1
**State:** pending

### Task 3: Valid task
**State:** pending
"#,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-multiple-parse-errors",
    );

    assert!(
        !result.status.success(),
        "expected parse failure\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stderr.contains("PARSE ERROR"));
    assert!(result.stderr.contains("2 problems"));
    assert!(result.stderr.contains("line 5"));
    assert!(result.stderr.contains("line 8"));
    assert!(result.stderr.contains("missing mandatory **State:**"));
    assert!(result.stderr.contains("Malformed metadata field"));
    assert!(!result.stderr.contains("VALIDATION ERROR"));
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
            &["Task plan.1", "has invalid state", "blocked", "Allowed:"],
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
        &[&["Task plan.1", "depends on missing Task plan.99"]],
        &["failed to parse", "Malformed task heading"],
    );
}

#[test]
fn cli_validate_reports_parent_as_prior_as_semantic_failure() {
    let result = run_validate(
        fixtures::INVALID_FIXTURE_PARENT_AS_PRIOR,
        fixtures::TEST_STATE_MACHINE,
        "integration-cli-parent-as-prior",
    );

    assert_validation_failure(
        &result,
        &[&["Task plan.fetch-prs.ci-failure-5227", "cannot list ancestor", "plan.fetch-prs", "Prior"]],
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

/// An unrecognized `**Field:**` in a task's metadata block is a parse error,
/// not content: swallowing it dropped mistyped `**Prior:**` lines into a plan
/// that validated green and ran out of order. §FS-rhei-plan-language.2
#[test]
fn unknown_metadata_field_in_the_metadata_block_is_rejected() {
    let result = run_validate(
        "# Rhei: Typo\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\
         **Priorr:** Task 2\n\n### Task 2: Two\n**State:** pending\n",
        fixtures::TEST_STATE_MACHINE,
        "unknown-metadata-field",
    );

    assert!(!result.status.success(), "a mistyped metadata field should fail validation");
    let stderr = normalize_for_assertions(&result.stderr);
    assert!(
        stderr.contains("Unknown metadata field '**Priorr:**'"),
        "error should name the unknown field, got:\n{stderr}"
    );
    assert!(
        stderr.contains("**State:**") && stderr.contains("**Prior:**"),
        "error should list the recognized fields, got:\n{stderr}"
    );
}

/// The same line past a blank line is ordinary bold prose and stays content.
#[test]
fn bold_label_in_task_content_is_not_a_metadata_field() {
    let result = run_validate(
        "# Rhei: Prose\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\n\
         **Owner:** alice\n",
        fixtures::TEST_STATE_MACHINE,
        "bold-label-content",
    );

    assert!(
        result.status.success(),
        "bold text after a blank line is content\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// A single file declaring one id twice must not report "defined in both X and
/// X", and must point at the redeclaration.
#[test]
fn duplicate_id_in_one_file_names_the_line() {
    let result = run_validate(
        "# Rhei: Dup\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n\n\
         ### Task 1: Duplicate\n**State:** pending\n",
        fixtures::TEST_STATE_MACHINE,
        "duplicate-id-one-file",
    );

    assert!(!result.status.success(), "a duplicate id should fail validation");
    let stderr = normalize_for_assertions(&result.stderr);
    assert!(
        stderr.contains("declared twice in"),
        "error should say the id is declared twice in one file, got:\n{stderr}"
    );
    assert!(
        stderr.contains("line 8") && stderr.contains("### Task 1: Duplicate"),
        "error should point at the redeclaration, got:\n{stderr}"
    );
}
