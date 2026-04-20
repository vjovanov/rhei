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

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));
    assert!(rendered.contains("### Task 1: Greet World"));
    assert!(rendered.contains("Say hello to World."));

    fs::remove_dir_all(dir).expect("cleanup");
}
