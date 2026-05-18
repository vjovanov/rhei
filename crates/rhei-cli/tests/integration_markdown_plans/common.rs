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
#[path = "../../../rhei-core/tests/fixtures.rs"]
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
