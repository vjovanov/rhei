fn write_run_agent_settings(dir: &Path, settings: &str) {
    let settings_dir = dir.join(".rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(settings_dir.join("settings.json"), settings).expect("write settings");
}

fn make_run_agent_script_executable(path: &Path) {
    let mut perms = fs::metadata(path).expect("stat agent script").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod agent script");
    }
}

fn collect_run_agent_snapshot_manifests(dir: &Path) -> Vec<serde_json::Value> {
    fn visit(path: &Path, manifests: &mut Vec<serde_json::Value>) {
        for entry in fs::read_dir(path).unwrap_or_else(|_| panic!("read dir {}", path.display())) {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                visit(&path, manifests);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("manifest.json") {
                let text = fs::read_to_string(&path).expect("manifest text");
                manifests.push(serde_json::from_str(&text).expect("manifest json"));
            }
        }
    }

    let cache = dir.join(".rhei/cache/snapshots");
    if !cache.exists() {
        return Vec::new();
    }
    let mut manifests = Vec::new();
    visit(&cache, &mut manifests);
    manifests
}

fn assert_run_agent_snapshot(
    manifests: &[serde_json::Value],
    snapshot_name: &str,
    completion: &str,
) {
    assert!(
        manifests.iter().any(|manifest| {
            manifest.get("snapshot_name").and_then(serde_json::Value::as_str)
                == Some(snapshot_name)
                && manifest.get("completion").and_then(serde_json::Value::as_str)
                    == Some(completion)
        }),
        "expected snapshot {snapshot_name:?} with completion {completion:?}; manifests: {manifests:#?}"
    );
}

#[test]
fn run_spawns_agent_state_without_outputs() {
    let machine = r#"name: no-output-agent
version: 1
states:
  review:
    initial: true
    agent: fake
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: No Output Agent

## Tasks

### Task 1: Review without artifacts
**State:** review
"#;
    let settings = r#"{
  "agents": {
    "fake": {
      "command": ["bash", "./agent.sh"],
      "timeout": "5s"
    }
  }
}"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
mkdir -p runtime
printf invoked > runtime/agent-invoked.txt
"#;

    let dir = unique_temp_dir("run-agent-no-outputs");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should spawn the no-output agent and complete\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(dir.join("runtime/agent-invoked.txt").exists(), "agent command should run");
    assert!(
        result.stdout.contains("1 agent(s)"),
        "summary should count the spawned agent; got:\n{}",
        result.stdout
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_model_state_without_resolved_agent_fails_instead_of_callback_fallback() {
    let machine = r#"name: missing-model-agent
version: 1
models:
  - local
states:
  review:
    initial: true
    model: local
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Missing Model Agent

## Tasks

### Task 1: Review with model
**State:** review
"#;
    let settings = r#"{
  "models": {
    "local": {
      "provider": "test",
      "model": "local-model"
    }
  }
}"#;

    let dir = unique_temp_dir("run-model-missing-agent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should fail when model work has no agent transport\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("no agent configured for model 'local'"),
        "missing-agent error should be reported; got:\n{}",
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "review");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_program_exit_zero_missing_outputs_leaves_task_without_error() {
    let machine = r#"name: program-missing-output
version: 1
states:
  build:
    initial: true
    program: "true"
    outputs:
      - name: bundle
        path: runtime/bundle.txt
  completed:
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
"#;
    let plan = r#"# Rhei: Program Missing Output

## Tasks

### Task 1: Build
**State:** build
"#;

    let dir = unique_temp_dir("run-program-missing-output");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should not abort when a successful program misses outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("program exited 0 but required outputs are missing"),
        "missing-output warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "build");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_agent_exit_zero_missing_outputs_emits_failure_snapshots() {
    let machine = r#"name: agent-missing-output-snapshot
version: 1
states:
  build:
    initial: true
    target: fake:openai:model
    outputs:
      - name: artifact
        path: runtime/artifact.txt
    snapshot:
      emit:
        name: failure
        on: failure
  completed:
    final: true
transitions:
  - from: build
    to: completed
"#;
    let plan = r#"# Rhei: Agent Missing Output Snapshot

## Tasks

### Task 1: Build
**State:** build
"#;
    let settings = r#"{
  "agents": {
    "fake": {
      "command": ["bash", "./agent.sh"],
      "timeout": "5s",
      "session": {
        "session_dir_flag": "--session-dir",
        "layout": { "kind": "FlatById", "ext": "jsonl" }
      }
    }
  }
}"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
session_dir=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-dir" ]; then session_dir="$2"; shift 2; else shift; fi
done
mkdir -p "$session_dir"
printf '{"provider":"openai","model":"model"}\n' > "$session_dir/session.jsonl"
"#;

    let dir = unique_temp_dir("run-agent-missing-output-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should warn but not fail on missing outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("agent exited 0 but required outputs are missing"),
        "missing-output warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "failure");
    assert_run_agent_snapshot(&manifests, "failure", "failure");
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "build");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_agent_exit_zero_without_transition_emits_always_snapshots() {
    let machine = r#"name: agent-no-transition-snapshot
version: 1
states:
  review:
    initial: true
    target: fake:openai:model
    snapshot:
      emit:
        name: always
        on: always
  completed:
    final: true
"#;
    let plan = r#"# Rhei: Agent No Transition Snapshot

## Tasks

### Task 1: Review
**State:** review
"#;
    let settings = r#"{
  "agents": {
    "fake": {
      "command": ["bash", "./agent.sh"],
      "timeout": "5s",
      "session": {
        "session_dir_flag": "--session-dir",
        "layout": { "kind": "FlatById", "ext": "jsonl" }
      }
    }
  }
}"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
session_dir=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-dir" ]; then session_dir="$2"; shift 2; else shift; fi
done
mkdir -p "$session_dir"
printf '{"provider":"openai","model":"model"}\n' > "$session_dir/session.jsonl"
"#;

    let dir = unique_temp_dir("run-agent-no-transition-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should halt because the task cannot advance\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("agent exited 0 but task 1 did not advance from 'review'"),
        "no-transition warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "success");
    assert_run_agent_snapshot(&manifests, "always", "success");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_agent_nonzero_exit_without_route_emits_failure_snapshot_before_abort() {
    let machine = r#"name: agent-error-snapshot
version: 1
states:
  work:
    initial: true
    target: fake:openai:model
    snapshot:
      emit:
        name: failure
        on: failure
  failed:
    final: true
"#;
    let plan = r#"# Rhei: Agent Error Snapshot

## Tasks

### Task 1: Work
**State:** work
"#;
    let settings = r#"{
  "agents": {
    "fake": {
      "command": ["bash", "./agent.sh"],
      "timeout": "5s",
      "session": {
        "session_dir_flag": "--session-dir",
        "layout": { "kind": "FlatById", "ext": "jsonl" }
      }
    }
  }
}"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
session_dir=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-dir" ]; then session_dir="$2"; shift 2; else shift; fi
done
mkdir -p "$session_dir"
printf '{"provider":"openai","model":"model"}\n' > "$session_dir/session.jsonl"
exit 7
"#;

    let dir = unique_temp_dir("run-agent-error-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        !result.status.success(),
        "run should abort after the nonzero exit\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "failure");
    assert_run_agent_snapshot(&manifests, "failure", "failure");
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "work");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_agent_timeout_route_emits_timeout_snapshot_before_transition() {
    let machine = r#"name: agent-timeout-snapshot
version: 1
states:
  work:
    initial: true
    target: fake:openai:model
    snapshot:
      emit:
        name: failure
        on: failure
  timed_out:
    final: true
transitions:
  - from: work
    to: timed_out
    timeout: 1s
"#;
    let plan = r#"# Rhei: Agent Timeout Snapshot

## Tasks

### Task 1: Work
**State:** work
"#;
    let settings = r#"{
  "agents": {
    "fake": {
      "command": ["bash", "./agent.sh"],
      "timeout": "1s",
      "session": {
        "session_dir_flag": "--session-dir",
        "layout": { "kind": "FlatById", "ext": "jsonl" }
      }
    }
  }
}"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
session_dir=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--session-dir" ]; then session_dir="$2"; shift 2; else shift; fi
done
mkdir -p "$session_dir"
printf '{"provider":"openai","model":"model"}\n' > "$session_dir/session.jsonl"
trap 'exit 143' TERM
sleep 30 &
wait
"#;

    let dir = unique_temp_dir("run-agent-timeout-snapshot");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should apply the timeout route\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let manifests = collect_run_agent_snapshot_manifests(&dir);
    assert_run_agent_snapshot(&manifests, "_state", "timeout");
    assert_run_agent_snapshot(&manifests, "failure", "timeout");
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "timed_out");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_spawns_all_fanout_invocations_for_selected_task_despite_parallel_one() {
    let machine = r#"name: fanout-parallel-task-limit
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    agent: fake
    all_models: [alpha, beta]
    outputs:
      - name: model-output
        path: runtime/{model}.txt
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan = r#"# Rhei: Fanout Parallel Limit

## Tasks

### Task 1: Review across models
**State:** review
"#;
    let settings = r#"{
  "agents": {
    "fake": {
      "command": ["bash", "./agent.sh"],
      "timeout": "5s"
    }
  },
  "models": {
    "alpha": {
      "provider": "test",
      "model": "alpha-model"
    },
    "beta": {
      "provider": "test",
      "model": "beta-model"
    }
  }
}"#;
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
: "${RHEI_MODEL:?RHEI_MODEL must be set}"
mkdir -p runtime
printf '%s\n' "$RHEI_MODEL" >> runtime/models.txt
printf done > "runtime/$RHEI_MODEL.txt"
"#;

    let dir = unique_temp_dir("run-fanout-parallel-one");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks", "--parallel", "1"]);

    assert!(
        result.status.success(),
        "run should spawn every fanout invocation for the selected task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("2 agent(s)"),
        "summary should count both fanout invocations; got:\n{}",
        result.stdout
    );
    assert!(dir.join("runtime/alpha.txt").exists(), "alpha output should exist");
    assert!(dir.join("runtime/beta.txt").exists(), "beta output should exist");
    let models = fs::read_to_string(dir.join("runtime/models.txt")).expect("read model log");
    assert!(models.lines().any(|line| line == "alpha"), "alpha invocation should run");
    assert!(models.lines().any(|line| line == "beta"), "beta invocation should run");

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    let task = rhei.tasks.iter().find(|task| task.id == TaskId::number(1)).expect("task");
    assert_eq!(task.state.as_str(), "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}
