//! §FS-rhei-errors: every failing command says what to do next, and every
//! command it prints survives a paste into an interactive shell.

use std::fs;
use std::process::Command;

use super::*;

fn run_raw(args: &[&str], cwd: &std::path::Path) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
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

/// A template with two required inputs and one execution-target input, so the
/// tests can exercise both the missing-input report and the format check.
fn write_agent_template(dir: &std::path::Path) {
    let template_dir = dir.join(".agents/rhei/templates/guided");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: guided
version: 1.0.0
description: Template used to exercise error guidance
inputs:
  - name: subject
    description: What to work on
    type: string
    required: true

  - name: brief
    description: How to work on it
    type: string
    required: true

  - name: agent
    description: Agent target that does the work
    type: string
    format: execution-target
    default: claude-code[yolo]:anthropic:claude-opus-4-7
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: {{subject}}

{{brief}}

## Tasks

### Task 1: Work on {{subject}}
**State:** pending
"#,
    );
}

#[test]
fn missing_inputs_are_reported_together_with_a_runnable_command() {
    let dir = unique_temp_dir("errors-missing-inputs");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // §FS-rhei-errors.1.1: both missing inputs in one report, not one per run.
    assert!(
        result.stderr.contains("missing 2 required inputs"),
        "expected both inputs reported at once; got:\n{}",
        result.stderr
    );
    assert!(result.stderr.contains("subject"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("brief"), "got:\n{}", result.stderr);

    // §FS-rhei-errors.1.2: the help line is a command, with the placeholders
    // quoted so `<value>` is not read as a shell redirection.
    assert!(
        result.stderr.contains("rhei instantiate guided subject='<value>' brief='<value>'"),
        "expected a runnable suggestion; got:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("rhei instantiate guided --list-inputs"),
        "expected a pointer to --list-inputs; got:\n{}",
        result.stderr
    );
}

#[test]
fn missing_input_suggestion_echoes_arguments_already_supplied() {
    let dir = unique_temp_dir("errors-missing-echo");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "subject=docs", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    assert!(
        result.stderr.contains("missing 1 required input"),
        "expected the singular form; got:\n{}",
        result.stderr
    );
    // §FS-rhei-errors.1.2: the suggestion carries what the user already typed.
    assert!(
        result.stderr.contains("rhei instantiate guided subject=docs brief='<value>'"),
        "expected supplied arguments to be echoed; got:\n{}",
        result.stderr
    );
}

#[test]
fn unknown_input_names_the_near_miss() {
    let dir = unique_temp_dir("errors-unknown-input");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "subjekt=docs", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    // §FS-rhei-errors.1.3
    assert!(result.stderr.contains("has no input named 'subjekt'"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("Did you mean 'subject'?"), "got:\n{}", result.stderr);
}

#[test]
fn unknown_template_names_the_near_miss() {
    let dir = unique_temp_dir("errors-unknown-template");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guidedd", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    assert!(result.stderr.contains("Did you mean 'guided'?"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("rhei templates"), "got:\n{}", result.stderr);
}

#[test]
fn malformed_execution_target_fails_against_the_input_the_user_typed() {
    let dir = unique_temp_dir("errors-execution-target");
    write_agent_template(&dir);

    let result = run_raw(
        &["instantiate", "guided", "subject=a", "brief=b", "agent=codex", "--dry-run"],
        &dir,
    );
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // §FS-rhei-errors.3.1: the error names the input, not the rendered file.
    assert!(
        result.stderr.contains("input 'agent' is not a valid execution target: 'codex'"),
        "expected the failure to name the input; got:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("states.yaml"),
        "the error must not point at a rendered file the user never wrote; got:\n{}",
        result.stderr
    );
    // §FS-rhei-errors.2: the suggested selector is quoted, because `[yolo]` is
    // a glob in zsh and would fail before rhei ever ran.
    assert!(
        result.stderr.contains("agent='codex[yolo]:openai:gpt-5.5'"),
        "expected a shell-quoted example selector; got:\n{}",
        result.stderr
    );
}

#[test]
fn list_inputs_quotes_values_that_a_shell_would_glob() {
    let dir = unique_temp_dir("errors-list-inputs");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--list-inputs"], &dir);
    assert_success(&result);
    // §FS-rhei-errors.2: this listing is where users copy values from.
    assert!(
        result.stdout.contains("default='claude-code[yolo]:anthropic:claude-opus-4-7'"),
        "expected the default selector to be quoted; got:\n{}",
        result.stdout
    );
    assert!(result.stdout.contains("format: execution-target"), "got:\n{}", result.stdout);
    assert!(
        result.stdout.contains("rhei instantiate guided subject='<value>' brief='<value>'"),
        "expected the listing to end on a runnable command; got:\n{}",
        result.stdout
    );
}

#[test]
fn missing_file_help_distinguishes_a_typo_from_a_missing_directory() {
    let dir = unique_temp_dir("errors-missing-file");

    // §FS-rhei-errors.6: the directory exists, so the remedy is to look at it.
    let result = run_raw(&["validate", "absent.rhei.md"], &dir);
    assert!(!result.status.success(), "validate should fail: {}", result.stdout);
    assert!(result.stderr.contains("Check the spelling"), "got:\n{}", result.stderr);

    // The directory is missing too, so the remedy is to create it.
    let result = run_raw(&["validate", "no/such/dir/absent.rhei.md"], &dir);
    assert!(!result.status.success(), "validate should fail: {}", result.stdout);
    assert!(result.stderr.contains("mkdir -p"), "got:\n{}", result.stderr);
}

#[test]
fn json_errors_carry_the_same_help_as_text_errors() {
    let dir = unique_temp_dir("errors-json");

    // §FS-rhei-errors.5
    let result = run_raw(&["list", "absent.rhei.md", "--json"], &dir);
    assert!(!result.status.success(), "list should fail: {}", result.stdout);
    let payload: serde_json::Value =
        serde_json::from_str(result.stderr.trim()).expect("stderr should be one JSON object");
    assert!(
        payload["error"]["message"].as_str().is_some_and(|msg| msg.contains("absent.rhei.md")),
        "got:\n{}",
        result.stderr
    );
    assert!(
        payload["error"]["help"].as_str().is_some_and(|help| help.contains("Check the spelling")),
        "expected the help to travel with the JSON error; got:\n{}",
        result.stderr
    );
}

#[test]
fn suggested_commands_are_never_wrapped_mid_command() {
    let dir = unique_temp_dir("errors-no-wrap");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    // §FS-rhei-errors.2: the renderer must not break `--list-inputs` across
    // lines, however narrow the terminal is reported to be.
    assert!(
        !result.stderr.contains("--list-\ninputs"),
        "the suggested command was wrapped; got:\n{}",
        result.stderr
    );
}
