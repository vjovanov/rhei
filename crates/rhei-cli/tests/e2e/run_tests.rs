use std::fs;

use super::*;

#[test]
fn run_builtin_default_refuses_manual_pending_tasks() {
    let dir = unique_temp_dir("run-builtin-manual-pending");
    let plan = r#"# Rhei: Manual Default

## Tasks

### Task 1: Do manually
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);

    let result = run_cli_without_machine("run", &plan_path, &[]);
    assert!(
        !result.status.success(),
        "run should refuse manual-only built-in pending task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "Task plan.1 is in manual-only initial state 'pending'");
    assert_stderr_contains(&result, "rhei next");
    assert_stderr_contains(&result, "rhei complete");
    let content = fs::read_to_string(&plan_path).expect("read plan after failed run");
    assert!(
        content.contains("**State:** pending") && !content.contains("**State:** completed"),
        "run must not complete manual task; got:\n{}",
        content
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-run.4: `--dry-run` exists to show what would happen. Aborting on
/// the first manual-only task made it fail before printing anything under the
/// built-in machine, whose initial state is manual-only.
#[test]
fn run_dry_run_reports_every_manual_only_task_instead_of_aborting() {
    let dir = unique_temp_dir("run-dry-run-manual-pending");
    let plan = r#"# Rhei: Manual Default

## Tasks

### Task 1: Do manually
**State:** pending

### Task 2: Also manually
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);

    let result = run_cli_without_machine("run", &plan_path, &["--dry-run", "--no-tui"]);
    assert!(
        !result.status.success(),
        "a dry run that found manual-only tasks still exits non-zero\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    for id in ["plan.1", "plan.2"] {
        assert!(
            combined.contains(&format!("manual-only: Task {id}")),
            "the scan must continue and report {id}; got:\n{combined}"
        );
    }
    assert!(
        combined.contains("rhei next") && combined.contains("rhei complete"),
        "the report must name the manual worker loop; got:\n{combined}"
    );

    let content = fs::read_to_string(&plan_path).expect("read plan after dry run");
    assert!(
        !content.contains("**State:** completed"),
        "a dry run must not rewrite the plan; got:\n{content}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_single_file_linear_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("run-linear", LINEAR_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("Running plan 'Linear Chain' with 3 task(s)"),
        "expected run header; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Pass 1: 1 ready, 0 terminal, 3 total."),
        "expected pass summary; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Final states: completed=3"),
        "expected final state summary; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions (2 per task); got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("3/3 tasks in terminal state"),
        "expected all tasks terminal; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_writes_durable_report_and_points_at_it() {
    // §FS-rhei-run-report.1: every run leaves a durable Markdown report and the
    // non-TTY output gains a greppable `Report:` pointer.
    let (dir, plan_path, machine_path) = setup_single_file("run-report", LINEAR_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert!(
        result.stdout.contains("Report: runtime/run-report.md"),
        "expected non-TTY Report pointer; got:\n{}",
        result.stdout
    );

    let runtime = dir.join("runtime");
    let report = fs::read_to_string(runtime.join("run-report.md")).expect("durable report written");
    assert!(report.starts_with("# Run Report: Linear Chain"), "got:\n{report}");
    assert!(report.contains("## Transition Ledger"), "got:\n{report}");
    assert!(report.contains("## Task Final States"), "got:\n{report}");
    assert!(report.contains("Result: completed"), "got:\n{report}");

    let history: Vec<_> = fs::read_dir(runtime.join("run-reports"))
        .expect("history dir exists")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(history.len(), 1, "one timestamped history entry written");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_single_file_parallel_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("run-parallel", PARALLEL_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_single_file_independent_to_completion() {
    let (dir, plan_path, machine_path) = setup_single_file("run-independent", INDEPENDENT_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_workspace_linear_to_completion() {
    let (ws, machine_path) = create_workspace(
        "run-ws-linear",
        "# Rhei: Workspace Linear\n",
        &[
            ("a.md", "### Task 1: First\n**State:** draft\n"),
            ("b.md", "### Task 2: Second\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Third\n**State:** draft\n**Prior:** Task 2\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    // Verify via CLI render.
    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    // Verify individual task files contain the updated state.
    for name in &["a.md", "b.md", "c.md"] {
        let content = fs::read_to_string(ws.join("tasks").join(name)).expect("read task file");
        assert!(
            content.contains("**State:** completed"),
            "{} should contain completed state: {}",
            name,
            content
        );
    }

    assert!(
        result.stdout.contains("6 transition(s) made"),
        "expected 6 transitions; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn run_workspace_parallel_to_completion() {
    let (ws, machine_path) = create_workspace(
        "run-ws-parallel",
        "# Rhei: Workspace Parallel\n",
        &[
            ("a.md", "### Task 1: Root\n**State:** draft\n"),
            ("b.md", "### Task 2: Branch A\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Branch B\n**State:** draft\n**Prior:** Task 1\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn run_bash_agent_team_fixture_to_completion() {
    let (dir, workspace_path, machine_path) =
        copy_workspace_fixture("run-bash-agent-team", "bash-agent-team");

    assert!(
        workspace_path.starts_with(repo_root().join("scratchpad")),
        "fixture copy should live under the shared gitignored scratchpad"
    );

    let result = run_cli("run", &workspace_path, &machine_path, &[]);
    assert_success(&result);

    assert_all_tasks_in_state(&workspace_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("6/6 tasks in terminal state"),
        "expected all tasks terminal; got:\n{}",
        result.stdout
    );

    let team_log =
        fs::read_to_string(workspace_path.join("runtime/logs/team.log")).expect("read team log");
    assert!(
        team_log.contains("mock kickoff command executed"),
        "expected kickoff log entry; got:\n{}",
        team_log
    );
    assert!(
        team_log.contains("reviewer finalized task"),
        "expected finalize log entry; got:\n{}",
        team_log
    );

    // Callbacks receive the project-qualified id, so ticket-keyed artifact
    // paths carry it — the same key space a narrowed reset matches on.
    for task_id in &["bash-agent-team.1", "bash-agent-team.2", "bash-agent-team.3"] {
        let artifact_dir = workspace_path.join(format!("runtime/artifacts/task-{task_id}"));
        assert!(
            artifact_dir.join("40-complete.txt").exists(),
            "task {} should have a completion artifact",
            task_id
        );
    }

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_living_review_loop_fixture_to_completion() {
    let (dir, workspace_path, machine_path) =
        copy_workspace_fixture("run-living-review-loop", "living-review-loop");

    let result = run_cli("run", &workspace_path, &machine_path, &["--no-agent"]);
    assert_success(&result);

    assert_all_tasks_in_state(&workspace_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("Workspace expanded: discovered 3 new task(s)"),
        "expected dynamic workspace expansion output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("6/6 tasks in terminal state"),
        "expected dynamically expanded tasks to complete; got:\n{}",
        result.stdout
    );

    let findings = fs::read_to_string(workspace_path.join("runtime/findings/review-findings.md"))
        .expect("read findings file");
    assert!(
        findings.contains("## Model claude"),
        "expected consolidated findings file; got:\n{}",
        findings
    );

    let verify_irrelevant =
        fs::read_to_string(workspace_path.join("runtime/verifications/F-002.md"))
            .expect("read verification file");
    assert!(
        verify_irrelevant.contains("- Relevant: no"),
        "expected non-relevant verification outcome; got:\n{}",
        verify_irrelevant
    );

    assert!(
        !workspace_path.join("tasks/13-fix-cli-help.md").exists(),
        "non-relevant finding should not produce a fix task"
    );
    assert!(
        workspace_path.join("tasks/11-fix-cache-key.md").exists(),
        "relevant finding F-001 should produce a fix task"
    );
    assert!(
        workspace_path.join("tasks/12-fix-timeout-details.md").exists(),
        "relevant finding F-003 should produce a fix task"
    );

    let team_log =
        fs::read_to_string(workspace_path.join("runtime/logs/team.log")).expect("read team log");
    assert!(
        team_log.contains("spawned verification tasks"),
        "expected review expansion in team log; got:\n{}",
        team_log
    );
    assert!(
        team_log.contains("spawned a fix task"),
        "expected selective fix expansion in team log; got:\n{}",
        team_log
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_applies_task_model_and_target_overrides_to_agent_processes() {
    let dir = unique_temp_dir("run-task-execution-overrides");
    let agent_script = write_fixture_file(
        &dir,
        "mock-agent.sh",
        r#"#!/bin/sh
set -eu
workspace="$(dirname "$RHEI_PLAN_PATH")"
mkdir -p "$workspace/runtime/logs"
printf 'task=%s model=%s target=%s mode=%s agent=%s provider=%s name=%s\n' \
  "${RHEI_TASK_ID:-}" "${RHEI_MODEL:-}" "${RHEI_TARGET:-}" "${RHEI_AGENT_MODE:-}" \
  "${RHEI_AGENT:-}" "${RHEI_MODEL_PROVIDER:-}" "${RHEI_MODEL_NAME:-}" \
  >> "$workspace/runtime/logs/override-agent.log"
"#,
    );
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": ["sh", {script_json}],
      "timeout": "5s",
      "modes": {{
        "yolo": [],
        "slow": []
      }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }},
    "special-model": {{ "provider": "mock", "model": "special-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: task-overrides
version: 1
models: [default-model, special-model]
states:
  work:
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let plan = r#"# Rhei: Task Overrides

## Tasks

### Task model-override: Use a task model
**State:** work
**Model:** special-model

### Task target-override: Use a task target
**State:** work
**Target:** mock[slow]:mock:target-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let log =
        fs::read_to_string(dir.join("runtime/logs/override-agent.log")).expect("read override log");
    assert!(
        log.contains(
            "task=plan.model-override model=special-model target=mock[yolo]:mock:special-model mode=yolo agent=mock provider=mock name=special-model"
        ),
        "model override did not preserve state target identity with swapped model; log:\n{}",
        log
    );
    assert!(
        log.contains(
            "task=plan.target-override model=target-model target=mock[slow]:mock:target-model mode=slow agent=mock provider=mock name=target-model"
        ),
        "target override did not replace full identity; log:\n{}",
        log
    );
    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_rejects_task_override_on_fanout_state() {
    let dir = unique_temp_dir("validate-task-override-fanout");
    let machine = r#"name: task-overrides-fanout
version: 1
models: [default-model, special-model]
states:
  review:
    all_models: [default-model, special-model]
    agent: codex
  completed:
    final: true
"#;
    let plan = r#"# Rhei: Task Override Fanout

## Tasks

### Task 1: Invalid override
**State:** review
**Model:** special-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(
        !result.status.success(),
        "validate should reject task override on fanout state\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let normalized_stderr = result.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        normalized_stderr.contains("Task plan.1 declares a task execution override")
            && normalized_stderr.contains("fanout state"),
        "expected fanout validation error; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_uses_task_override_for_transition_output_artifact_checks() {
    let dir = unique_temp_dir("run-task-override-output-checks");
    let agent_script = write_fixture_file(
        &dir,
        "mock-agent.sh",
        r#"#!/bin/sh
set -eu
workspace="$(dirname "$RHEI_PLAN_PATH")"
mkdir -p "$workspace/runtime/outputs"
printf 'model=%s\n' "${RHEI_MODEL:-}" > "$workspace/runtime/outputs/${RHEI_MODEL}.txt"
"#,
    );
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": ["sh", {script_json}],
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }},
    "special-model": {{ "provider": "mock", "model": "special-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: output-checks
version: 1
models: [default-model, special-model]
states:
  work:
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
    outputs:
      - name: model-output
        path: runtime/outputs/{model}.txt
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let plan = r#"# Rhei: Task Override Output Checks

## Tasks

### Task 1: Use special output
**State:** work
**Model:** special-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(dir.join("runtime/outputs/special-model.txt").exists());
    assert!(!dir.join("runtime/outputs/default-model.txt").exists());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_does_not_create_agent_work_from_task_override_in_callback_state() {
    let dir = unique_temp_dir("run-task-override-callback-only");
    let agent_script = write_fixture_file(
        &dir,
        "mock-agent.sh",
        r#"#!/bin/sh
set -eu
workspace="$(dirname "$RHEI_PLAN_PATH")"
mkdir -p "$workspace/runtime/logs"
printf 'unexpected agent spawn\n' >> "$workspace/runtime/logs/agent.log"
"#,
    );
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": ["sh", {script_json}],
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: callback-only
version: 1
states:
  step: {}
  completed:
    final: true
transitions:
  - from: step
    to: completed
"#;
    let plan = r#"# Rhei: Callback Only Override

## Tasks

### Task 1: Callback transition
**State:** step
**Target:** mock[yolo]:mock:target-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(
        !dir.join("runtime/logs/agent.log").exists(),
        "task override should not spawn an agent for callback-only states"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_cli_model_override_supersedes_task_target_model() {
    let dir = unique_temp_dir("run-cli-model-over-task-target");
    let agent_script = write_fixture_file(
        &dir,
        "mock-agent.sh",
        r#"#!/bin/sh
set -eu
workspace="$(dirname "$RHEI_PLAN_PATH")"
mkdir -p "$workspace/runtime/logs"
printf 'model=%s target=%s\n' "${RHEI_MODEL:-}" "${RHEI_TARGET:-}" \
  >> "$workspace/runtime/logs/agent.log"
"#,
    );
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": ["sh", {script_json}],
      "timeout": "5s",
      "modes": {{
        "yolo": [],
        "slow": []
      }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }},
    "cli-model": {{ "provider": "mock", "model": "cli-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");

    let machine = r#"name: cli-model-over-target
version: 1
models: [default-model, cli-model]
states:
  work:
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let plan = r#"# Rhei: CLI Model Over Task Target

## Tasks

### Task 1: Target with CLI model
**State:** work
**Target:** mock[slow]:mock:task-target-model
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "run",
        &plan_path,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--model", "cli-model"],
    );
    assert_success(&result);

    let log = fs::read_to_string(dir.join("runtime/logs/agent.log")).expect("read agent log");
    assert!(
        log.contains("model=cli-model target=mock[slow]:mock:cli-model"),
        "CLI model override should replace the task target model segment; log:\n{}",
        log
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_executes_program_states_and_routes_on_exit_code() {
    let plan = r#"# Rhei: Program State Run

## Tasks

### Task 1: Build artifact
**State:** build
"#;
    let machine = r#"name: program-demo
version: 1
states:
  build:
    description: Build the artifact
    program: "mkdir -p runtime && echo ok > runtime/program-1.txt"
  completed:
    description: Done
    final: true
  failed:
    description: Failed
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
  - from: build
    to: failed
    exit_code: nonzero
"#;

    let dir = unique_temp_dir("run-program-state");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    assert!(
        dir.join("runtime/program-1.txt").exists(),
        "program should have produced its output artifact"
    );
    assert!(
        result.stdout.contains("program(s) spawned"),
        "expected program summary in output; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_counted_self_loop_terminates_at_visit_budget() {
    // Regression: under `rhei run`, a counted state that loops to ITSELF
    // (`tick -> tick`) used to spin forever. The orchestrator compared the
    // reloaded raw state (`tick-2`) against the normalized current state
    // (`tick`) and mistook the visit suffix for forward progress, skipping the
    // real transition logic — so `visitCount` never advanced past 2 and the
    // `visitCount >= visits` exit could never fire.
    let plan = r#"# Rhei: Counted Self Loop

## Tasks

### Task 1: Tick
**State:** tick
"#;
    let machine = r#"name: counted-self-loop
version: 1
states:
  tick:
    initial: true
    description: Counted program self-loop
    program: "true"
    visits: 3
  done:
    description: Done
    final: true
transitions:
  - { from: tick, to: tick, condition: visitCount < visits }
  - { from: tick, to: done, condition: visitCount >= visits }
"#;

    let dir = unique_temp_dir("run-counted-self-loop");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "done");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_callback_mode_stops_at_human_review() {
    let plan = r#"# Rhei: Human Review Gate

## Tasks

### Task 1: Aggregate findings
**State:** aggregate
"#;
    let machine = r#"name: human-review-gate
version: 1
states:
  aggregate:
    initial: true
    description: Aggregate findings
  human-review:
    description: Wait for a human decision
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: aggregate
    to: human-review
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir("run-human-review-gate");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "human-review");
    assert!(
        !result.stdout.contains("'human-review' → 'completed'"),
        "run should stop at the gating state; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_callback_mode_waits_for_other_branches_before_halting_at_human_review() {
    let plan = r#"# Rhei: Human Review Barrier

## Tasks

### Task 1: Human gate
**State:** aggregate

### Task 2: Independent cleanup
**State:** work

### Task 3: After approval
**State:** work
**Prior:** Task 1
"#;
    let machine = r#"name: human-review-barrier
version: 1
states:
  aggregate:
    description: Aggregate findings
  work:
    description: Ordinary autonomous work
  human-review:
    description: Wait for a human decision
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: aggregate
    to: human-review
  - from: work
    to: completed
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir("run-human-review-barrier");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "human-review");
    assert_task_state(&plan_path, &machine_path, "2", "completed");
    assert_task_state(&plan_path, &machine_path, "3", "work");
    assert!(
        !result.stdout.contains("Task plan.1 transitioned: 'human-review' → 'completed'"),
        "gating task must not advance autonomously; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Task plan.2 transitioned: 'work' → 'completed'"),
        "independent non-gating work should still complete before the run halts; got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn changeset_review_human_review_state_is_gating_in_shipped_workflows() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let example_path = repo_root.join("examples/changeset-review-example/states.yaml");
    let example_yaml = fs::read_to_string(&example_path).expect("read example states.yaml");
    let machine = rhei_validator::StateMachine::from_yaml_str(&example_yaml)
        .unwrap_or_else(|err| panic!("parse {}: {err}", example_path.display()));
    let human_review = machine
        .states
        .get("human-review")
        .unwrap_or_else(|| panic!("{} missing human-review state", example_path.display()));
    assert!(human_review.gating, "{} should mark human-review as gating", example_path.display());
    assert!(
        machine
            .transitions
            .iter()
            .any(|rule| rule.from.0 == "decide" && rule.to.0 == "human-review"),
        "{} should route final decisions through human-review",
        example_path.display()
    );
    assert!(
        machine
            .transitions
            .iter()
            .any(|rule| rule.from.0 == "human-review" && rule.to.0 == "prepare-workspace"),
        "{} should require human approval before workspace preparation",
        example_path.display()
    );

    let template_path = repo_root.join("crates/rhei-cli/templates/changeset-review/states.yaml");
    let template = fs::read_to_string(&template_path).expect("read template states.yaml");
    let start = template
        .find("\n  human-review:\n")
        .unwrap_or_else(|| panic!("{} missing human-review block", template_path.display()));
    let end = template[start + 1..]
        .find("\n  fix-spawn:\n")
        .map(|offset| start + 1 + offset)
        .unwrap_or(template.len());
    let human_review_block = &template[start..end];
    assert!(
        human_review_block.contains("\n    gating: true\n"),
        "{} should mark human-review as gating",
        template_path.display()
    );
    assert!(
        template.contains("\n  - from: decide\n    to: human-review\n"),
        "{} should route final decisions through human-review",
        template_path.display()
    );
    assert!(
        template.contains("\n  - from: human-review\n    to: prepare-workspace\n")
            && template.contains("\n  - from: human-review\n    to: final-fix\n"),
        "{} should require human approval before either fix path",
        template_path.display()
    );
}

#[test]
fn run_prefers_agent_mode_for_model_declared_workflows_without_falling_back_to_callbacks() {
    let (ws, machine_path) = create_workspace(
        "run-model-declared-agent-mode",
        "# Rhei: Review Workflow\n",
        &[("task.md", "### Task coordinate: Coordinate review\n**State:** split\n")],
    );

    let machine = r#"name: review-workflow
version: 1
models:
  - codex
states:
  split:
    initial: true
    description: Coordinator
    instructions: Write `{output.overview.path}`.
    outputs:
      - name: overview
        path: runtime/overview.md
  review:
    description: Review
    model: codex
  completed:
    final: true
    description: Done
transitions:
  - from: split
    to: completed
  - from: review
    to: completed
"#;
    fs::write(&machine_path, machine).expect("write machine");

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks"]);
    assert!(
        !result.status.success(),
        "run should fail without a configured agent transport\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("no agent configured"),
        "expected explicit missing-agent error; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !result.stderr.contains("Missing required output artifact"),
        "run should not fall back to callback-only output validation; got:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn reset_bash_agent_team_fixture_restores_initial_state() {
    let (dir, workspace_path, machine_path) =
        copy_workspace_fixture("reset-bash-agent-team", "bash-agent-team");
    let source_fixture = fixture_path("bash-agent-team");

    let run_result = run_cli("run", &workspace_path, &machine_path, &[]);
    assert_success(&run_result);

    // §FS-rhei-reset.1.2: no terminal here, so the intent is stated explicitly.
    let reset_result = run_cli("reset", &workspace_path, &machine_path, &["-y"]);
    assert_success(&reset_result);

    assert_all_tasks_in_state(&workspace_path, &machine_path, "pending");
    assert!(
        !workspace_path.join("runtime").exists(),
        "reset should remove generated runtime output"
    );

    for task_file in &["01-brief.md", "02-research.md", "03-implementation.md"] {
        let actual = fs::read_to_string(workspace_path.join("tasks").join(task_file))
            .expect("read reset task file");
        let expected = fs::read_to_string(source_fixture.join("tasks").join(task_file))
            .expect("read source fixture task file");
        assert_eq!(actual, expected, "{} should match the checked-in fixture", task_file);
    }

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_partially_advanced_completes_remaining() {
    let plan = r#"# Rhei: Partial Advance

## Tasks

### Task 1: Already done
**State:** completed

### Task 2: Needs work
**State:** draft
**Prior:** Task 1

### Task 3: Also needs work
**State:** draft
**Prior:** Task 2
"#;

    let (dir, plan_path, machine_path) = setup_single_file("run-partial", plan);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");
    assert!(
        result.stdout.contains("4 transition(s) made"),
        "expected 4 transitions (2 each for Tasks 2 & 3); got:\n{}",
        result.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_already_completed_is_noop() {
    let plan = r#"# Rhei: All Done

## Tasks

### Task 1: Done
**State:** completed

### Task 2: Also done
**State:** completed
**Prior:** Task 1
"#;

    let (dir, plan_path, machine_path) = setup_single_file("run-noop", plan);
    let original = fs::read_to_string(&plan_path).expect("read plan");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    assert!(
        result.stdout.contains("No tasks could be advanced"),
        "expected no-op message; got:\n{}",
        result.stdout
    );

    let after = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(original, after, "file should be unchanged");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_parallel_does_not_warn_for_a_ticket_with_subtasks_in_one_file() {
    // §FS-rhei-run.2.5: only top-level tickets count toward a file — a ticket
    // and its subtasks are one schedulable unit, not shared-file concurrency.
    let (ws, machine_path) = create_workspace(
        "run-parallel-subtasks",
        "# Rhei: Subtask Layout\n**States:** integration-test\n",
        &[
            (
                "one.md",
                "### Task 1: Alpha\n**State:** draft\n\n#### Task 1.1: Detail\n**State:** draft\n",
            ),
            ("two.md", "### Task 2: Beta\n**State:** draft\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert!(
        !result.stderr.contains("schedules tickets from the same rhei file"),
        "a ticket plus its subtasks is not a shared file:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Falling back to sequential"),
        "a multi-file plan keeps parallelism:\n{}",
        result.stderr
    );

    fs::remove_dir_all(ws.parent().expect("workspace parent")).expect("cleanup");
}

#[test]
fn run_parallel_warns_when_one_of_several_files_owns_two_tickets() {
    let (ws, machine_path) = create_workspace(
        "run-parallel-shared",
        "# Rhei: Shared File\n**States:** integration-test\n",
        &[
            (
                "one.md",
                "### Task 1: Alpha\n**State:** draft\n\n### Task 2: Beta\n**State:** draft\n",
            ),
            ("two.md", "### Task 3: Gamma\n**State:** draft\n"),
        ],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert!(
        result.stderr.contains("schedules tickets from the same rhei file")
            && result.stderr.contains("one.md"),
        "two top-level tickets in one of several files should warn and name it:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Falling back to sequential"),
        "other files still benefit from parallelism:\n{}",
        result.stderr
    );

    fs::remove_dir_all(ws.parent().expect("workspace parent")).expect("cleanup");
}

#[test]
fn run_parallel_falls_back_to_sequential_when_all_tickets_share_one_file() {
    // §FS-rhei-run.2.5: with every ticket in one plan file, parallel slots
    // could only schedule same-file tickets — sequential, as for a bare file.
    let (ws, machine_path) = create_workspace(
        "run-parallel-single-file",
        "# Rhei: One File\n**States:** integration-test\n",
        &[(
            "one.md",
            "### Task 1: Alpha\n**State:** draft\n\n### Task 2: Beta\n**State:** draft\n",
        )],
    );

    let result = run_cli("run", &ws, &machine_path, &["--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert!(
        result.stderr.contains("Falling back to sequential execution"),
        "a single ticket-owning file cannot run in parallel:\n{}",
        result.stderr
    );

    fs::remove_dir_all(ws.parent().expect("workspace parent")).expect("cleanup");
}

/// `rhei run` and `rhei next` share one eligibility rule, so the orchestrator
/// schedules a parent only after the subtree it integrates is terminal. The
/// central ledger records the order, which is what proves it.
// §FS-rhei-run.3 §FS-rhei-plan-language.3
#[test]
fn run_schedules_a_parent_only_after_its_subtree_is_terminal() {
    let plan = r#"# Rhei: Parent Scheduling

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: First subtask
**State:** draft

#### Task 1.2: Second subtask
**State:** draft

### Task 2: Dependent
**State:** draft
**Prior:** Task 1
"#;
    let (dir, plan_path, machine_path) = setup_single_file("run-parent-order", plan);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks"]);
    assert_success(&result);
    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    let ledger = fs::read_to_string(dir.join("runtime").join("state-transitions.log"))
        .expect("read the central transition ledger");
    let position = |line: &str| {
        ledger
            .lines()
            .position(|entry| entry == line)
            .unwrap_or_else(|| panic!("ledger should contain {line:?}; got:\n{ledger}"))
    };
    let parent_done = position("plan.1 pending@completed");
    for child in ["plan.1.1 pending@completed", "plan.1.2 pending@completed"] {
        assert!(
            position(child) < parent_done,
            "the parent must not be stamped terminal before {child}; got:\n{ledger}"
        );
    }
    assert!(
        parent_done < position("plan.2 draft@pending"),
        "the dependent must not start before the parent is terminal; got:\n{ledger}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

const PARENT_GATE_MACHINE: &str = r#"name: parent-gate
version: 1
states:
  draft:
    initial: true
    description: Start
  gate:
    description: Waiting on a human decision
    gating: true
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: completed
  - from: gate
    to: completed
"#;

/// The narrowing this rule buys: a parent with an open descendant is neither
/// scheduled nor stamped terminal, so the run cannot leave behind a plan that
/// fails `rhei validate`. The halt report names the parent and what holds it.
///
/// The run exits zero: one gate is holding the whole branch, and a gate is the
/// plan working as authored, not a failure to report.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn run_leaves_a_parent_alone_while_a_descendant_is_gated() {
    let plan = r#"# Rhei: Gated Subtree

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Gated subtask
**State:** gate
"#;
    let dir = unique_temp_dir("run-parent-gated-child");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PARENT_GATE_MACHINE);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "draft");
    let content = fs::read_to_string(&plan_path).expect("read plan after run");
    assert!(
        content.contains("#### Task 1.1: Gated subtask\n**State:** gate"),
        "the gated descendant must be untouched; got:\n{content}"
    );
    assert!(
        combined.contains("Task plan.1 (draft): waiting on open descendant Task plan.1.1 (gate)"),
        "the halt report must name the parent and the descendant holding it; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.1.1 (gate): gating state awaiting review"),
        "the descendant must still report its own cause; got:\n{combined}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The same plan plus one dependent of the parent. A `**Prior:**` that resolves
/// to a parent held open by a gated child inherits that gate: the run is
/// waiting on one human decision, not stalled, so it still exits zero and still
/// leaves every ticket where it found it.
///
/// Judging the prior by its own stored state instead saw a non-gating parent
/// with no priors of its own, read the plan as unadvanceable, and exited one.
// §FS-rhei-run.3 §FS-rhei-plan-language.3 §FS-rhei-run-report.3.1
#[test]
fn run_leaves_a_dependent_of_a_gate_held_parent_alone() {
    let plan = r#"# Rhei: Gated Subtree With Dependent

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Gated subtask
**State:** gate

### Task 2: Dependent task
**State:** draft
**Prior:** Task 1
"#;
    let dir = unique_temp_dir("run-parent-gated-child-dependent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", PARENT_GATE_MACHINE);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert_success(&result);

    assert_task_state(&plan_path, &machine_path, "1", "draft");
    assert_task_state(&plan_path, &machine_path, "2", "draft");
    let content = fs::read_to_string(&plan_path).expect("read plan after run");
    assert!(
        content.contains("#### Task 1.1: Gated subtask\n**State:** gate"),
        "the gated descendant must be untouched; got:\n{content}"
    );
    assert!(
        combined.contains("Task plan.1 (draft): waiting on open descendant Task plan.1.1 (gate)"),
        "the halt report must name the parent and the descendant holding it; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.2 (draft): waiting on Task plan.1"),
        "the dependent must name the parent it waits on; got:\n{combined}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
