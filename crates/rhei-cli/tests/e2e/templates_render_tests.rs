//! Instantiation *rendering*: the restricted MiniJinja environment — what a
//! template author may write in a skeleton, and what comes out the other side.
//! The `rhei instantiate` / `rhei templates` surface is in `templates_tests.rs`.

// §FS-rhei-templates.5

use std::fs;

use super::templates_tests::run_raw;
use super::*;

/// §FS-rhei-templates.5: `range()`, arithmetic, and `~` unroll a counted
/// structure into one task per round, with the per-round `**Prior:**` metadata
/// that a `visits:` loop cannot express because it repeats one task.
#[test]
fn instantiate_unrolls_rounds_with_range_and_arithmetic() {
    let dir = unique_temp_dir("templates-range");
    let template_dir = dir.join("rounds-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: rounds-template
version: 1.0.0
description: Template that unrolls its rounds
inputs:
  - name: review_rounds
    description: How many review rounds to unroll
    type: number
    default: 3
"#,
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        r#"name: rounds-template
version: 1.0.0
states:
  review:
    initial: true
    description: Review round
    agent: claude-code
    visits: {{ 2 * review_rounds + 1 }}
    instructions: Review round {visit_count}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: review, to: completed, description: Round done }
  - { from: "*", to: cancelled, description: Dropped }
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Rounds
**States:** rounds-template

## Tasks
{% for k in range(1, review_rounds + 1) %}
### Task {{ "review-" ~ k }}: Review round {{k}} of {{review_rounds}}
**State:** review
{% if k > 1 %}**Prior:** Task review-{{ k - 1 }}
{% endif %}
{%- endfor %}
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

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    // `range(1, n + 1)` is half-open at the top, so the rounds are 1..=n.
    for k in 1..=3 {
        assert!(
            rendered.contains(&format!("### Task review-{k}: Review round {k} of 3")),
            "round {k} should be unrolled; got:\n{rendered}"
        );
    }
    assert!(!rendered.contains("review-4"), "range stops before the bound; got:\n{rendered}");
    // `k - 1` names the previous round, so each round waits on the one before
    // it — the per-task metadata a counted `visits:` loop has no place to put.
    assert!(
        rendered.contains(
            "### Task review-2: Review round 2 of 3\n**State:** review\n**Prior:** Task review-1"
        ),
        "round 2 waits on round 1; got:\n{rendered}"
    );
    assert!(
        rendered.contains("### Task review-1: Review round 1 of 3\n**State:** review\n\n"),
        "the first round has no prior; got:\n{rendered}"
    );

    // Arithmetic sizes the state's own budget from the same input.
    let machine =
        fs::read_to_string(output_dir.join("states.yaml")).expect("read rendered machine");
    assert!(machine.contains("visits: 7"), "2 * 3 + 1; got:\n{machine}");
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
    // The repro command renders the output path relative to the working
    // directory it is pasted from. §FS-rhei-templates.6.1.3

    // The two absolute paths are matched on their own, because the printed
    // command is quoted for a shell and a Windows path's backslashes make it a
    // quoted word.
    assert!(
        result.stdout.contains(&template_dir.display().to_string())
            && result.stdout.contains(&dir.join("values.yaml").display().to_string()),
        "the repro command names the template and its values file; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("--output output"),
        "expected values-file instantiate command in output; got:\n{}",
        result.stdout
    );

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("- claude => claude-code-yolo-anthropic-claude-opus-4-7"));
    assert!(rendered.contains("- gemini => gemini-yolo-google-gemini-3.1-pro-preview"));
}
