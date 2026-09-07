// A freed parallel slot is a fresh scheduling decision over a freshly loaded
// task. Its agent identity must therefore include the task metadata just as the
// initial fill and serial scheduler do. §FS-rhei-run.5 §FS-rhei-plan-language.3.11

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::*;

#[derive(Debug)]
struct ObservedIdentity {
    worker: String,
    header: BTreeMap<String, String>,
    started_ns: u128,
}

fn run_target_override_case(prefix: &str, parallel: &str) -> BTreeMap<String, ObservedIdentity> {
    let index = r#"# Rhei: Parallel target override

Three independent tasks force an initial two-slot fill and a later refill.
"#;
    let tasks = [
        (
            "01-fast.md",
            r#"### Task fast: Fast initial task
**State:** work
**Target:** requested-agent[slow]:requested-provider:requested-model
"#,
        ),
        (
            "02-hold.md",
            r#"### Task hold: Slow initial task
**State:** work
**Target:** requested-agent[slow]:requested-provider:requested-model
"#,
        ),
        (
            "03-refill.md",
            r#"### Task refill: Freed-slot refill task
**State:** work
**Target:** requested-agent[slow]:requested-provider:requested-model
"#,
        ),
    ];
    let (dir, workspace, machine_path) = create_workspace(prefix, index, &tasks);
    let agent_script = write_python_agent(
        &dir,
        "identity-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
write(root / 'runtime' / 'starts' / (local + '.txt'), str(time.time_ns()))
time.sleep(0.3 if local == 'hold' else 0.03)
result('## Result\n\nIdentity observed.\n')
"#,
    );
    let command = fixture_command(&agent_script);
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "requested-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "slow": [] }}
    }},
    "fallback-agent": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }}
}}"#
        ),
    )
    .expect("write settings");
    fs::write(
        &machine_path,
        r#"name: parallel-target-override
version: 1
states:
  work:
    concurrent: true
    target: fallback-agent[yolo]:fallback-provider:fallback-model
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#,
    )
    .expect("write state machine");

    let result = run_cli(
        "run",
        &workspace,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--parallel", parallel],
    );
    assert_success(&result);

    let mut observed = BTreeMap::new();
    for entry in fs::read_dir(workspace.join("runtime/spawns")).expect("read spawn records") {
        let path = entry.expect("spawn record entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let record: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).expect("read spawn record"))
                .expect("spawn record parses");
        let task = record["task"].as_str().expect("spawn record task");
        let local = task.rsplit('.').next().expect("local task id");
        let worker = record["worker"].as_str().expect("spawn record worker").to_string();
        let log_path = Path::new(record["log"].as_str().expect("spawn record log"));
        let header = fs::read_to_string(log_path)
            .expect("read spawn log")
            .lines()
            .filter_map(|line| line.split_once(": "))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        let started_ns =
            fs::read_to_string(workspace.join("runtime/starts").join(format!("{local}.txt")))
                .expect("read agent start observation")
                .parse()
                .expect("agent start observation is nanoseconds");
        observed.insert(local.to_string(), ObservedIdentity { worker, header, started_ns });
    }
    assert_eq!(observed.len(), 3, "one spawn record per task: {observed:#?}");
    observed
}

fn identity_mismatches(case: &str, observed: &BTreeMap<String, ObservedIdentity>) -> Vec<String> {
    let expected = [
        ("worker", "requested-agent"),
        ("agent", "requested-agent"),
        ("mode", "slow"),
        ("target", "requested-agent[slow]:requested-provider:requested-model"),
        ("provider", "requested-provider"),
        ("model", "requested-model"),
        ("model_name", "requested-model"),
    ];
    let mut mismatches = Vec::new();
    for local in ["fast", "hold", "refill"] {
        let identity = observed.get(local).unwrap_or_else(|| panic!("missing {case} {local}"));
        for (field, expected_value) in expected {
            let actual = if field == "worker" {
                Some(identity.worker.as_str())
            } else {
                identity.header.get(field).map(String::as_str)
            };
            if actual != Some(expected_value) {
                mismatches.push(format!(
                    "{case} {local} {field}: expected {expected_value:?}, got {actual:?}"
                ));
            }
        }
    }
    mismatches
}

#[test]
fn parallel_refill_preserves_the_tasks_full_target_override() {
    let parallel = run_target_override_case("parallel-target-override", "2");
    let serial = run_target_override_case("serial-target-override", "1");

    let refill_started = parallel["refill"].started_ns;
    assert!(
        refill_started > parallel["fast"].started_ns
            && refill_started > parallel["hold"].started_ns,
        "refill must start after both initial-fill tasks: {parallel:#?}"
    );

    let mut mismatches = identity_mismatches("parallel", &parallel);
    mismatches.extend(identity_mismatches("serial control", &serial));
    assert!(
        mismatches.is_empty(),
        "task target override was not preserved:\n{}",
        mismatches.join("\n")
    );
}
