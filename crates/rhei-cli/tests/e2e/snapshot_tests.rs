use std::fs;
use std::path::Path;
use std::process::Command;

use super::*;

fn run_snapshot_command(plan_path: &Path, machine_path: &Path, args: &[&str]) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", isolated_home_for(plan_path))
        .arg("--state-machine")
        .arg(machine_path)
        .arg("snapshot");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei snapshot command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn write_fake_snapshot_agent(dir: &Path) -> String {
    let script = dir.join("fake-snapshot-agent.sh");
    fs::write(
        &script,
        r#"#!/bin/sh
session_dir=""
resume_value=""
interactive=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --interactive)
      interactive=1
      ;;
    --session-dir)
      shift
      session_dir="${1:-}"
      ;;
    --resume)
      shift
      resume_value="${1:-}"
      ;;
    --prompt)
      shift
      ;;
    --model)
      shift
      ;;
  esac
  shift || true
done

runtime_root="${RHEI_ROOT:-.}/runtime"
mkdir -p "$runtime_root"
{
  printf 'task=%s state=%s target=%s resume=%s parent=%s\n' \
    "$RHEI_TASK_ID" "$RHEI_STATE" "$RHEI_TARGET_SLUG" "$resume_value" \
    "${RHEI_SNAPSHOT_PARENT_REF:-}"
} >> "$runtime_root/fake-agent.log"

if [ -n "$session_dir" ]; then
  mkdir -p "$session_dir"
  session_id="${RHEI_TASK_ID}-${RHEI_STATE}-${RHEI_TARGET_SLUG:-target}"
  {
    printf '{"session":{"provider":"%s","model":"%s"}}\n' \
      "${RHEI_MODEL_PROVIDER:-acme}" "${RHEI_MODEL_NAME:-model-a}"
    printf '{"role":"assistant","content":"%s","interactive":%s}\n' \
      "$RHEI_STATE" "$interactive"
  } > "$session_dir/$session_id.jsonl"
fi
"#,
    )
    .expect("write fake agent script");
    script.display().to_string()
}

#[test]
fn snapshot_cli_lists_shows_and_run_preloads_from_snapshot() {
    let dir = unique_temp_dir("snapshot-cli-smoke");
    let fake_agent = write_fake_snapshot_agent(&dir);
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agents/rhei");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "fake": {{
      "command": ["sh", {}],
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "session": {{
        "resume": {{"flag": "--resume"}},
        "interactive": {{"args": ["--interactive"]}},
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }}
  }}
}}"#,
            serde_json::to_string(&fake_agent).expect("json string")
        ),
    )
    .expect("write settings");

    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Snapshot CLI Smoke

## Tasks

### Task 1: Carry context
**State:** source
"#,
    );
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: snapshot-cli-smoke
version: 1
states:
  source:
    initial: true
    description: Produce a reusable snapshot
    target: fake:acme:model-a
    snapshot:
      emit:
        name: impl
        on: always
  review:
    description: Consume the implementation snapshot
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: impl
        required: true
        select:
          state: source
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: review
  - from: review
    to: completed
"#,
    );

    let plan_arg = plan_path.to_string_lossy().to_string();
    let empty_list =
        run_snapshot_command(&plan_path, &machine_path, &["list", "--plan", &plan_arg]);
    assert_success(&empty_list);
    assert!(
        empty_list.stdout.contains("No snapshots found")
            && empty_list.stdout.contains("--produced-by all"),
        "expected empty snapshot list to guide the operator; got:\n{}",
        empty_list.stdout
    );
    let empty_json = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json"],
    );
    assert_success(&empty_json);
    assert_eq!(empty_json.stdout.trim(), "[]");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let list = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&list);
    let rows: serde_json::Value =
        serde_json::from_str(&list.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["target_slug"] == "fake-acme-model-a"
                && row["current"] == true
        }),
        "expected current named source snapshot in list; got:\n{}",
        list.stdout
    );
    assert!(
        rows.iter()
            .any(|row| row["snapshot_name"] == "_state" && row["emitting_state"] == "review"),
        "expected auto-emitted review snapshot in list; got:\n{}",
        list.stdout
    );

    let snapshot_ref = "1:impl:source@1:fake-acme-model-a/g1";
    let show = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["show", snapshot_ref, "--plan", &plan_arg],
    );
    assert_success(&show);
    assert!(
        show.stdout.contains("\"snapshot_name\": \"impl\"")
            && show.stdout.contains("\"emitting_state\": \"source\"")
            && show.stdout.contains("\"session_id\": \"1-source-fake-acme-model-a\"")
            && show.stdout.contains("transcript preview:"),
        "expected snapshot show to print manifest and transcript preview; got:\n{}",
        show.stdout
    );

    fs::write(
        &plan_path,
        r#"# Rhei: Snapshot CLI Smoke

## Tasks

### Task 1: Carry context
**State:** review
"#,
    )
    .expect("rewind task to inherited state");
    let from_snapshot =
        run_cli("run", &plan_path, &machine_path, &["--no-tui", "--from-snapshot", snapshot_ref]);
    assert_success(&from_snapshot);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let agent_log = fs::read_to_string(dir.join("runtime/fake-agent.log")).expect("agent log");
    assert!(
        agent_log.contains("state=review")
            && agent_log.contains("resume=1-source-fake-acme-model-a")
            && agent_log.contains("\"snapshot_name\":\"impl\""),
        "expected inherited run to preload the selected snapshot; got:\n{}",
        agent_log
    );

    let continued = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["continue", snapshot_ref, "--plan", &plan_arg],
    );
    assert_success(&continued);
    assert!(
        continued.stdout.contains("captured 1:impl:source@1:fake-acme-model-a/g2")
            && continued.stdout.contains("--produced-by operator"),
        "expected snapshot continue to capture an operator generation; got:\n{}",
        continued.stdout
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let operator = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["show", "1:impl:source@1:fake-acme-model-a/g2", "--plan", &plan_arg],
    );
    assert_success(&operator);
    assert!(
        operator.stdout.contains("\"produced_by\": \"operator\"")
            && operator.stdout.contains("\"completion\": \"success\"")
            && operator.stdout.contains("\"parent_ref\"")
            && operator.stdout.contains("\"generation\": 1"),
        "expected operator generation to record success and source parent_ref; got:\n{}",
        operator.stdout
    );

    let after_continue = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&after_continue);
    let rows: serde_json::Value =
        serde_json::from_str(&after_continue.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["generation"] == 1
                && row["produced_by"] == "orchestrator"
                && row["current"] == true
        }) && rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["generation"] == 2
                && row["produced_by"] == "operator"
                && row["current"] == false
        }),
        "expected operator generation to leave current on orchestrator generation; got:\n{}",
        after_continue.stdout
    );

    let no_capture = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["continue", snapshot_ref, "--plan", &plan_arg, "--no-capture"],
    );
    assert_success(&no_capture);
    assert!(
        no_capture.stdout.contains("without capture; no snapshot written"),
        "expected --no-capture to confirm no generation was written; got:\n{}",
        no_capture.stdout
    );
    let after_no_capture = run_snapshot_command(
        &plan_path,
        &machine_path,
        &["list", "--plan", &plan_arg, "--format", "json", "--produced-by", "all"],
    );
    assert_success(&after_no_capture);
    let rows: serde_json::Value =
        serde_json::from_str(&after_no_capture.stdout).expect("snapshot list json should parse");
    let rows = rows.as_array().expect("snapshot list should be an array");
    assert!(
        !rows.iter().any(|row| {
            row["snapshot_name"] == "impl"
                && row["emitting_state"] == "source"
                && row["generation"] == 3
        }),
        "--no-capture must not write another generation; got:\n{}",
        after_no_capture.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
