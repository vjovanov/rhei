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

#[test]
fn templates_lists_project_local_templates() {
    let dir = unique_temp_dir("templates-list");
    let template_dir = dir.join(".agents/rhei/templates/hello");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello
version: 1.0.0
description: Simple hello-world template
inputs:
  - name: target
    description: Greeting target
  - name: punctuation
    description: Greeting suffix
    required: false
    default: "!"
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let result = run_raw(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("hello"),
        "expected template name in output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("inputs: target, punctuation?"),
        "expected input summary in output; got:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(&template_dir.display().to_string()),
        "expected short template name only, without template path; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_without_template_lists_available_templates() {
    let dir = unique_temp_dir("templates-instantiate-list");
    let template_dir = dir.join(".agents/rhei/templates/hello");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello
version: 1.0.0
description: Simple hello-world template
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello

## Tasks

### Task 1: Greet
**State:** pending
"#,
    );

    let result = run_raw(&["instantiate"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("Templates:") && result.stdout.contains("hello  1.0.0  project"),
        "expected instantiate without template to list templates; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_unknown_template_suggests_close_match() {
    let dir = unique_temp_dir("templates-instantiate-suggest");
    let template_dir = dir.join(".agents/rhei/templates/code-review");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: code-review
version: 1.0.0
description: Review code changes
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Code Review

## Tasks

### Task 1: Review
**State:** pending
"#,
    );

    let result = run_raw(&["instantiate", "code-reveiw"], &dir);
    assert!(
        !result.status.success(),
        "command should fail for unknown template\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("Did you mean 'code-review'?"),
        "expected close template suggestion; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_renders_template_variables_and_validates_output() {
    let dir = unique_temp_dir("templates-instantiate");
    let template_dir = dir.join("hello-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello-template
version: 1.0.0
description: Simple hello-world template
inputs:
  - name: target
    description: Greeting target
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending

Say hello to {{target}}.
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "target=World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("Instantiate this template with:"),
        "expected instantiate hint in output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(&format!(
            "rhei instantiate {} --set target=World --output {}",
            template_dir.display(),
            output_dir.display()
        )),
        "expected reproducible instantiate command in output; got:\n{}",
        result.stdout
    );

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));
    assert!(rendered.contains("### Task 1: Greet World"));
    assert!(rendered.contains("Say hello to World."));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_prints_output_tree_task_tail_and_stop_reason() {
    let dir = unique_temp_dir("templates-instantiate-summary");
    let template_dir = dir.join("summary-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: summary-template
version: 1.0.0
description: Template with enough tasks to exercise the summary
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Summary Demo

## Tasks

### Task 1: Step 1
**State:** draft

### Task 2: Step 2
**State:** draft

Body for step 2.

### Task 3: Step 3
**State:** draft

### Task 4: Step 4
**State:** draft

### Task 5: Step 5
**State:** draft

### Task 6: Step 6
**State:** draft

Body for step 6.
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("=== Instantiation Summary ===")
            && result.stdout.contains("Files:\n"),
        "expected pretty instantiate summary in output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("`-- plan.rhei.md"),
        "expected materialized file tree in instantiate output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Task tree:\n  - Task 1: Step 1 [draft]"),
        "expected task tree in instantiate output; got:\n{}",
        result.stdout
    );
    let last_tasks = result
        .stdout
        .split("Recent task definitions:\n")
        .nth(1)
        .and_then(|section| section.split("Stopped:\n").next())
        .unwrap_or_else(|| {
            panic!("expected Recent task definitions section; got:\n{}", result.stdout)
        });
    assert!(
        last_tasks.contains("--- Task 2: Step 2 [draft] ---")
            && last_tasks.contains("### Task 2: Step 2\n**State:** draft\n\nBody for step 2.")
            && last_tasks.contains("--- Task 6: Step 6 [draft] ---")
            && last_tasks.contains("### Task 6: Step 6\n**State:** draft\n\nBody for step 6.")
            && !last_tasks.contains("Task 1: Step 1 [draft]"),
        "expected the last five rendered task definitions, excluding task 1; got:\n{}",
        last_tasks
    );
    assert!(
        result.stdout.contains("Stopped:\n  instantiation stopped before execution; next ready task is Task 1: Step 1 [draft]."),
        "expected stop reason in instantiate output; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_project_hourly_human_intervention_template_prints_summary() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let dir = unique_temp_dir("templates-hourly-human-intervention");
    let output_dir = dir.join("hourly");

    let result = run_raw(
        &[
            "instantiate",
            "hourly-human-intervention",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &repo_root,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("=== Instantiation Summary ===")
            && result.stdout.contains("Files:")
            && result.stdout.contains(".agents/")
            && result.stdout.contains("Task tree:")
            && result.stdout.contains(
                "Task fetch-issues: Fetch and classify human-intervention issues [fetch]"
            )
            && result.stdout.contains("Recent task definitions:")
            && result.stdout.contains(
                "### Task fetch-prs: Fetch and classify human-intervention pull requests"
            )
            && result.stdout.contains(
                "Task fetch-prs: Fetch and classify human-intervention pull requests [fetch]"
            )
            && result.stdout.contains(
                "Task follow-up-rhei-prs: Follow up on RHEI pull requests [rhei-pr-follow-up]"
            )
            && result.stdout.contains("Stopped:"),
        "expected hourly template instantiation summary; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_accepts_manifest_declared_positional_input() {
    let dir = unique_temp_dir("templates-positional");
    let template_dir = dir.join("positional-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: positional-template
version: 1.0.0
description: Template with positional input
inputs:
  - name: target
    description: Greeting target
    positional: 1
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_execute_accepts_run_args_after_separator() {
    let dir = unique_temp_dir("templates-execute-run-args");
    let template_dir = dir.join("execute-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: execute-template
version: 1.0.0
description: Template that immediately executes
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Execute Template

## Tasks

### Task 1: Step
**State:** pending
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--execute",
            "--output",
            output_dir.to_str().expect("output path"),
            "--",
            "--dry-run",
            "--parallel",
            "3",
            "--no-agent",
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("Instantiated template 'execute-template'")
            && result.stdout.contains("Running plan 'Execute Template'"),
        "expected instantiation followed by run output; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !result.stderr.contains("does not accept positional inputs"),
        "run arguments after -- must not be treated as template inputs; got stderr:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_maps_single_required_input_to_one_bare_value() {
    let dir = unique_temp_dir("templates-single-required");
    let template_dir = dir.join("single-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: single-template
version: 1.0.0
description: Template with one required input
inputs:
  - name: target
    description: Greeting target
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_relocates_root_settings_json_into_agents_rhei_dir() {
    let dir = unique_temp_dir("templates-settings-bundling");
    let template_dir = dir.join("settings-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: settings-template
version: 1.0.0
description: Template that bundles project settings
inputs:
  - name: workspace_id
    description: Linear workspace id
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Bundled settings demo

## Tasks

### Task 1: Demo
**State:** pending
"#,
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "mcp_servers": {
    "linear": {
      "command": ["npx", "-y", "@modelcontextprotocol/server-linear"],
      "env": { "LINEAR_WORKSPACE": "{{workspace_id}}" }
    }
  }
}
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "workspace_id=acme-engineering",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    assert!(
        !output_dir.join("settings.json").exists(),
        "template settings.json should not be written at output root"
    );
    assert!(
        !output_dir.join(".rhei/settings.json").exists(),
        "template settings.json should not be written under .rhei"
    );
    let rendered_settings = fs::read_to_string(output_dir.join(".agents/rhei/settings.json"))
        .expect("read .agents/rhei/settings.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&rendered_settings).expect("rendered settings.json is valid JSON");
    assert_eq!(
        parsed["mcp_servers"]["linear"]["env"]["LINEAR_WORKSPACE"], "acme-engineering",
        "instantiation variable should be substituted in settings.json"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_renders_structured_inputs_with_minijinja_loops() {
    let dir = unique_temp_dir("templates-structured");
    let template_dir = dir.join("structured-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: structured-template
version: 1.0.0
description: Template with structured inputs
inputs:
  - name: targets
    description: Target list
    type: array
    items:
      type: object
      properties:
        id:
          type: string
        selector:
          type: string
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Structured

## Tasks

### Task analysis: Review targets
**State:** pending

{% for target in targets %}
- {{ target.id }} => {{ target.selector|slug }}
{% endfor %}
"#,
    );
    write_fixture_file(
        &dir,
        "values.yaml",
        r#"targets:
  - id: claude
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: gemini
    selector: gemini[yolo]:google:gemini-3.1-pro-preview
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--values",
            dir.join("values.yaml").to_str().expect("values path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains(&format!(
            "rhei instantiate {} --values {} --output {}",
            template_dir.display(),
            dir.join("values.yaml").display(),
            output_dir.display()
        )),
        "expected values-file instantiate command in output; got:\n{}",
        result.stdout
    );

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("- claude => claude-code-yolo-anthropic-claude-opus-4-7"));
    assert!(rendered.contains("- gemini => gemini-yolo-google-gemini-3.1-pro-preview"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_enforces_validate_on_nested_array_item_property() {
    let dir = unique_temp_dir("templates-nested-validate");
    let template_dir = dir.join("nested-validate-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: nested-validate-template
version: 1.0.0
description: Template with a validate on a nested array-item property
inputs:
  - name: targets
    description: Target list
    type: array
    items:
      type: object
      properties:
        id:
          type: string
          validate: "[a-z][a-z0-9-]*"
        path:
          type: string
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Nested validate

## Tasks

### Task review: Review targets
**State:** pending

{% for target in targets %}
- {{ target.id }} :: {{ target.path }}
{% endfor %}
"#,
    );

    // An id that violates the nested `validate` pattern must fail
    // instantiation, with an error that points at the offending nested path.
    write_fixture_file(&dir, "bad-values.yaml", "targets:\n  - id: Bad_ID\n    path: src\n");
    let bad = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--values",
            dir.join("bad-values.yaml").to_str().expect("values path"),
            "--output",
            dir.join("bad-output").to_str().expect("output path"),
        ],
        &dir,
    );
    assert!(
        !bad.status.success(),
        "expected instantiation to fail on invalid nested id; stdout:\n{}\nstderr:\n{}",
        bad.stdout,
        bad.stderr
    );
    let combined = format!("{}{}", bad.stdout, bad.stderr);
    assert!(
        combined.contains("targets[0].id")
            && combined.contains("does not match validation pattern"),
        "error should point at the nested property; got stdout:\n{}\nstderr:\n{}",
        bad.stdout,
        bad.stderr
    );

    // A valid id renders successfully.
    write_fixture_file(&dir, "good-values.yaml", "targets:\n  - id: backend\n    path: src\n");
    let good = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--values",
            dir.join("good-values.yaml").to_str().expect("values path"),
            "--output",
            dir.join("good-output").to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&good);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn instantiate_rejects_template_settings_json_with_malformed_render() {
    let dir = unique_temp_dir("templates-settings-malformed");
    let template_dir = dir.join("bad-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: bad-template
version: 1.0.0
description: Template with malformed settings.json
inputs:
  - name: key
    description: Arbitrary string
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Broken settings

## Tasks

### Task 1: Demo
**State:** pending
"#,
    );
    // Missing opening brace after rendering makes this invalid JSON.
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "mcp_servers": {
    "linear": "{{key}}"
  }
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "key=oops",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert!(
        !result.status.success(),
        "expected instantiation to fail on malformed settings.json; stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("settings.json") || result.stdout.contains("settings.json"),
        "error should mention settings.json; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
