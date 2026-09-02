//! Black-box contract coverage for published accounting schemas.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const SCHEMA_IDS: [&str; 6] = [
    "rhei.accounting.cost.v1",
    "rhei.accounting.invocation.v1",
    "rhei.accounting.prices.v1",
    "rhei.accounting.summary.v1",
    "rhei.accounting.task.v1",
    "rhei.accounting.usage.v1",
];

const ACCOUNTING_MACHINE: &str = r#"name: accounting-contract
version: 1
models: [contract]
states:
  work:
    initial: true
    description: Emit accounting artifacts
    agent: codex
    model: contract
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

const ACCOUNTING_PLAN: &str = r#"# Rhei: Accounting Contract

## Tasks

### Task 1: Produce one measured invocation
**State:** work
"#;

fn schema_output(home: &Path, schema_id: &str) -> CliRun {
    let output =
        rhei_command(home).args(["schema", schema_id]).output().expect("schema command runs");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn first_file(directory: &Path, extension: &str) -> PathBuf {
    let mut paths = fs::read_dir(directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 1, "expected one {extension} file under {}", directory.display());
    paths.remove(0)
}

fn validate(schema: &serde_json::Value, instance: &serde_json::Value, label: &str) {
    // Rhei's contract id is intentionally the same self-identifying token used
    // in artifacts, which is a relative URI reference. Validation has no
    // retrieval base or external references, so compile the same schema with
    // only that annotation removed.
    let mut schema_for_validation = schema.clone();
    schema_for_validation.as_object_mut().expect("schema object").remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema_for_validation)
        .unwrap_or_else(|err| panic!("compile schema for {label}: {err}"));
    if let Err(errors) = compiled.validate(instance) {
        let messages = errors.map(|error| error.to_string()).collect::<Vec<_>>().join("\n");
        panic!("{label} did not validate:\n{messages}\ninstance:\n{instance:#}");
    };
}

fn write_contract_agent_settings(root: &Path) {
    let script = write_python_agent(
        root,
        "accounting-contract-codex.py",
        r#"import json

print(json.dumps({'type': 'thread.started', 'thread_id': 'thread-contract-141'}), flush=True)
print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': 120,
        'cached_input_tokens': 20,
        'cache_creation_input_tokens': 5,
        'output_tokens': 30,
    },
}), flush=True)
result('## Result\n\nAccounting contract emitted.\n')
"#,
    );
    let settings_dir = root.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "codex", "model": "contract", "agent_timeout": "10s" }},
  "agents": {{
    "codex": {{ "command": {}, "prompt_flag": "--prompt", "timeout": "10s" }}
  }},
  "models": {{
    "contract": {{ "provider": "openai", "model": "gpt-contract", "default_agent": "codex" }}
  }}
}}"#,
            fixture_command(&script)
        ),
    )
    .expect("write settings");
}

/// Every published id lists and prints the exact versioned schema source; an
/// unknown id fails with a discovery hint. §FS-rhei-cost-accounting.8.1
#[test]
fn schema_command_lists_and_prints_every_published_contract() {
    let dir = unique_temp_dir("accounting-schema-command");
    let bare =
        rhei_command(dir.join("home")).arg("schema").output().expect("bare schema list runs");
    assert!(bare.status.success(), "{}", String::from_utf8_lossy(&bare.stderr));
    assert_eq!(String::from_utf8_lossy(&bare.stdout), SCHEMA_IDS.join("\n") + "\n");

    let output = rhei_command(dir.join("home"))
        .args(["schema", "--list"])
        .output()
        .expect("schema list runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), SCHEMA_IDS.join("\n") + "\n");

    for schema_id in SCHEMA_IDS {
        let result = schema_output(&dir.join("home"), schema_id);
        assert_success(&result);
        let source = fs::read_to_string(
            repo_root().join("crates/rhei-cli/schemas").join(format!("{schema_id}.schema.json")),
        )
        .expect("versioned schema source");
        assert_eq!(result.stdout, source);
        let parsed: serde_json::Value = serde_json::from_str(&result.stdout).expect("schema JSON");
        assert_eq!(parsed["$id"], schema_id);
    }

    let unknown = schema_output(&dir.join("home"), "rhei.accounting.unknown.v1");
    assert!(!unknown.status.success());
    assert_stderr_contains(&unknown, "unknown accounting schema id 'rhei.accounting.unknown.v1'");
    assert_stderr_contains(&unknown, "rhei schema --list");
}

/// One actual run produces all six artifact/output shapes accepted by their
/// published schemas, including exact CLI session identity and duration.
// §FS-rhei-cost-accounting.3.4 §FS-rhei-cost-accounting.8.1
#[test]
fn run_artifacts_and_cost_json_validate_against_published_schemas() {
    let dir = unique_temp_dir("accounting-schema-run");
    let plan = write_fixture_file(&dir, "plan.rhei.md", ACCOUNTING_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", ACCOUNTING_MACHINE);
    write_contract_agent_settings(&dir);

    let run = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&run);

    let accounting = dir.join("runtime/accounting");
    let invocation_path = first_file(&accounting.join("invocations"), "json");
    let task_path = first_file(&accounting.join("tasks"), "json");
    let capture_path = first_file(&accounting.join("captures"), "jsonl");
    let invocation: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(invocation_path).expect("invocation"))
            .expect("invocation JSON");
    assert_eq!(invocation["cli_session"]["id"], "thread-contract-141");
    assert!(invocation["cli_session"].get("store_path").is_none());
    assert!(invocation["duration_ms"].as_u64().is_some());

    let task_id = invocation["task_id"].as_str().expect("task id");
    let cost = run_cli("cost", &plan, &machine, &["--json", "--task", task_id]);
    assert_success(&cost);
    let cost_json: serde_json::Value = serde_json::from_str(&cost.stdout).expect("cost JSON");

    let artifacts = [
        ("rhei.accounting.invocation.v1", invocation),
        (
            "rhei.accounting.summary.v1",
            serde_json::from_str(
                &fs::read_to_string(accounting.join("summary.json")).expect("summary"),
            )
            .expect("summary JSON"),
        ),
        (
            "rhei.accounting.task.v1",
            serde_json::from_str(&fs::read_to_string(task_path).expect("task")).expect("task JSON"),
        ),
        (
            "rhei.accounting.prices.v1",
            serde_json::from_str(
                &fs::read_to_string(accounting.join("prices.json")).expect("prices"),
            )
            .expect("prices JSON"),
        ),
        (
            "rhei.accounting.usage.v1",
            serde_json::from_str(
                fs::read_to_string(capture_path)
                    .expect("usage capture")
                    .lines()
                    .next()
                    .expect("usage event"),
            )
            .expect("usage JSON"),
        ),
        ("rhei.accounting.cost.v1", cost_json),
    ];

    for (schema_id, artifact) in artifacts {
        let schema_result = schema_output(&dir.join("schema-home"), schema_id);
        assert_success(&schema_result);
        let schema: serde_json::Value =
            serde_json::from_str(&schema_result.stdout).expect("published schema JSON");
        validate(&schema, &artifact, schema_id);
        if schema_id == "rhei.accounting.invocation.v1" {
            let mut old_artifact = artifact.clone();
            let old_object = old_artifact.as_object_mut().expect("invocation object");
            old_object.remove("duration_ms");
            old_object.remove("cli_session");
            validate(&schema, &old_artifact, "pre-session invocation v1");
        }
    }
}
