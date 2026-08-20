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
mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
printf '## Result\n\nMock agent finished.\n' > "$RHEI_RESULT_PATH"
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
mkdir -p "$workspace/runtime/outputs" "$(dirname "$RHEI_RESULT_PATH")"
printf 'model=%s\n' "${RHEI_MODEL:-}" > "$workspace/runtime/outputs/${RHEI_MODEL}.txt"
printf '## Result\n\nMock agent finished.\n' > "$RHEI_RESULT_PATH"
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
mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
printf '## Result\n\nMock agent finished.\n' > "$RHEI_RESULT_PATH"
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
    program: >-
      mkdir -p runtime "$(dirname "$RHEI_RESULT_PATH")"
      && echo ok > runtime/program-1.txt
      && printf '## Result\n\nBuilt the artifact.\n' > "$RHEI_RESULT_PATH"
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
    program: >-
      mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
      && printf '## Result\n\nTicked.\n' > "$RHEI_RESULT_PATH"
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
    let machine = rhei_cli::rhei_validator::StateMachine::from_yaml_str(&example_yaml)
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

    // The prediction and the outcome are one judgment: a dry run that exits
    // non-zero where the real run exits zero is not a prediction.
    // §FS-rhei-run-report.3.1
    let dry =
        run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui", "--dry-run"]);
    assert_success(&dry);

    fs::remove_dir_all(dir).expect("cleanup");
}

/// One gate, two siblings: the second child's `**Prior:**` is the first, which
/// is the gated one. Every open ticket in the plan traces back to the same
/// human decision, so the run exits zero and leaves the plan untouched.
///
/// The verdict must not depend on which child the walk reaches first. Sharing
/// one visited set between the descendant walk and the prior walk made it: the
/// gate was consumed by the parent's descendant loop, so the sibling's prior
/// walk saw an already-visited node, scored it as no reason to wait, and the
/// whole branch read as stuck.
// §FS-rhei-run.3 §FS-rhei-plan-language.3 §FS-rhei-run-report.3.1
#[test]
fn run_leaves_a_sibling_of_a_gated_sibling_alone() {
    let plan = r#"# Rhei: Gated Sibling

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Gated subtask
**State:** gate

#### Task 1.2: Waits on the gated subtask
**State:** draft
**Prior:** Task 1.1
"#;
    let dir = unique_temp_dir("run-gated-sibling");
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
        content.contains("#### Task 1.2: Waits on the gated subtask\n**State:** draft"),
        "the sibling must be untouched; got:\n{content}"
    );
    assert!(
        combined.contains("Task plan.1 (draft): waiting on open descendant Task plan.1.1 (gate)"),
        "the parent must name the descendants holding it; got:\n{combined}"
    );
    assert!(
        combined.contains("Task plan.1.2 (draft): waiting on Task plan.1.1"),
        "the sibling must name the gated ticket it waits on; got:\n{combined}"
    );

    let dry =
        run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui", "--dry-run"]);
    assert_success(&dry);

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A ticket whose worker finished without moving it must not starve its
/// siblings, and must not be re-spawned into every slot that frees up.
///
/// Both failed in the parallel worker pool. `stalled_tasks` was declared
/// outside the pass loop and never cleared, and the exit-0-with-missing-outputs
/// arm `continue`d past the pool refill — so the slot the stalled worker had
/// just freed stayed idle for the rest of the pass, and the run halted with "no
/// progress" while four ready tickets had never been spawned at all. Sibling
/// arms that also fail to advance (an auto-advance error, a timeout with no
/// rule, a non-zero exit under `--continue-on-error`) did not mark the ticket
/// stalled either, so the live-ready-set refill re-spawned it inside one pass,
/// without bound.
// §FS-rhei-run.3
#[test]
fn a_stalled_ticket_does_not_starve_its_siblings_or_respawn_without_bound() {
    let dir = unique_temp_dir("run-parallel-stall-refill");
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Stall Refill\n").expect("write index");
    for n in 1..=6 {
        fs::write(
            tasks_dir.join(format!("{n:02}-item.md")),
            format!("### Task {n}: Item {n}\n**State:** work\n"),
        )
        .expect("write task file");
    }

    let agent_script = write_fixture_file(
        &dir,
        "mock-agent.sh",
        r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/logs" "$root/runtime/out"
printf '%s\n' "${RHEI_TASK_ID:-}" >> "$root/runtime/logs/spawns.log"
# Tickets 1 and 2 exit 0 having written nothing: they fail the completion
# condition and stay put. Everyone else finishes.
case "${RHEI_TASK_ID:-}" in
  workspace.1|workspace.2) exit 0 ;;
esac
printf 'done\n' > "$root/runtime/out/${RHEI_TASK_ID}.md"
mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
printf '## Result\n\nFinished.\n' > "$RHEI_RESULT_PATH"
"#,
    );
    let settings_dir = workspace.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "timeout": "10s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: stall-refill
version: 1
states:
  work:
    initial: true
    description: Do it
    agent: mock
    agent_timeout: 10s
    concurrent: true
    outputs:
      - name: out
        path: runtime/out/{task_id}.md
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#,
    );

    let result = run_cli(
        "run",
        &workspace,
        &machine_path,
        &["--no-tui", "--no-callbacks", "--parallel", "2"],
    );
    assert!(
        !result.status.success(),
        "two tickets cannot finish, so the run halts non-zero\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // The siblings queued behind the stalled pair were scheduled and finished.
    for n in 3..=6 {
        assert_task_state(&workspace, &machine_path, &n.to_string(), "completed");
    }
    assert_task_state(&workspace, &machine_path, "1", "work");
    assert_task_state(&workspace, &machine_path, "2", "work");

    // Each ticket is spawned at most once per pass. The stalled pair is retried
    // on the second (and last) pass — the set of stalled tickets is scoped to a
    // pass, not written off for the run — and before the fix it was re-spawned
    // into every slot that freed up, without bound, inside pass one.
    let spawns =
        fs::read_to_string(workspace.join("runtime/logs/spawns.log")).expect("read spawn log");
    for n in 1..=6 {
        let id = format!("workspace.{n}");
        let count = spawns.lines().filter(|line| line.trim() == id).count();
        let bound = if n <= 2 { 2 } else { 1 };
        assert!(
            (1..=bound).contains(&count),
            "{id}: spawned {count} time(s), expected between 1 and {bound}\n{spawns}"
        );
    }

    fs::remove_dir_all(dir).expect("cleanup");
}

// ---------------------------------------------------------------------------
// Supervised process groups: interruption, teardown, and the timeout that now
// takes the whole group with it.
// ---------------------------------------------------------------------------

// §FS-rhei-run.3.2: one termination path for every subprocess a run starts.

/// A fake agent that backgrounds a grandchild and then sleeps — the shape
/// issue #53 was reported with, where killing the direct child left the
/// grandchild running.
#[cfg(unix)]
const GRANDCHILD_AGENT: &str = r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/pids"
sleep 300 &
printf '%s\n' "$!" > "$root/runtime/pids/grandchild"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
sleep 300
"#;

/// A fake agent that is one process and dies only to a signal it cannot catch.
#[cfg(unix)]
const LONE_AGENT: &str = r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/pids"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
exec sleep 300
"#;

/// A fake agent that does its ticket's work and exits, so the run reaches its
/// own end and the TUI parks on the finished screen.
#[cfg(unix)]
const QUICK_AGENT: &str = r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/pids" "$(dirname "${RHEI_RESULT_PATH:?}")"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
printf '## Result\n\nMock agent finished.\n' > "$RHEI_RESULT_PATH"
exit 0
"#;

/// A fake agent that ignores `SIGTERM`, so only the `SIGKILL` at the end of the
/// grace — or a second interrupt that skips it — can end it.
#[cfg(unix)]
const STUBBORN_AGENT: &str = r#"#!/bin/sh
set -eu
trap '' TERM
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/pids"
printf '%s\n' "$$" > "$root/runtime/pids/agent"
exec sleep 300
"#;

/// A one-ticket workspace whose only state runs `agent_body`.
#[cfg(unix)]
fn setup_supervised_workspace(
    prefix: &str,
    agent_body: &str,
    agent_timeout: &str,
) -> (PathBuf, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Supervised\n").expect("write index");
    fs::write(tasks_dir.join("01-work.md"), "### Task 1: Work\n**State:** work\n")
        .expect("write task file");

    let agent_script = write_fixture_file(&dir, "mock-agent.sh", agent_body);
    let settings_dir = workspace.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "{agent_timeout}" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "timeout": "{agent_timeout}" }} }}
}}"#
        ),
    )
    .expect("write settings");

    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: supervised
version: 1
states:
  work:
    initial: true
    description: Do it
    agent: mock
    agent_timeout: {agent_timeout}
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
        ),
    );
    (dir, workspace, machine_path)
}

/// A live `rhei run` that dies with the test.
///
/// Every wait in these tests polls to a deadline and panics when it passes,
/// and a panic before the signal under test would leave `rhei`, its agent, and
/// the agent's `sleep 300` running for five minutes. `SIGKILL` to `rhei` is
/// enough on Linux — its subprocesses follow through the parent-death backstop
/// — but not on macOS, where a failed test may still leave an agent behind.
#[cfg(unix)]
struct KillOnDrop(std::process::Child);

#[cfg(unix)]
impl std::ops::Deref for KillOnDrop {
    type Target = std::process::Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(unix)]
impl std::ops::DerefMut for KillOnDrop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(unix)]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// Join a helper thread if it has already finished, and detach it otherwise:
/// a drain thread whose pty never closes must not decide how long a test runs.
#[cfg(unix)]
fn join_or_detach(handle: std::thread::JoinHandle<()>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !handle.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if handle.is_finished() {
        let _ = handle.join();
    }
}

/// Start `rhei run` as a live child so the test can signal it, with its output
/// on disk for the assertions and for the failure message.
#[cfg(unix)]
fn spawn_rhei_run(dir: &Path, workspace: &Path, machine: &Path) -> KillOnDrop {
    spawn_rhei_run_with(dir, workspace, machine, &[])
}

/// [`spawn_rhei_run`] with extra `run` flags.
#[cfg(unix)]
fn spawn_rhei_run_with(
    dir: &Path,
    target: &Path,
    machine: &Path,
    extra_args: &[&str],
) -> KillOnDrop {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", dir.join(".home"));
    cmd.arg("--state-machine")
        .arg(machine)
        .arg("run")
        .arg(target)
        .arg("--no-tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard");
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(fs::File::create(dir.join("run.out")).expect("create run stdout"));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    KillOnDrop(cmd.spawn().expect("rhei run should start"))
}

#[cfg(unix)]
fn signal_pid(pid: u32, signal: &str) {
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .status()
        .expect("kill should run");
    assert!(status.success(), "kill -{signal} {pid} failed");
}

/// Whether a pid still exists, by the `kill -0` rule.
#[cfg(unix)]
fn pid_is_alive(pid: &str) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Poll until `check` holds, so a slow machine costs patience rather than a
/// failure. Panics with `what` when the deadline passes.
#[cfg(unix)]
fn poll_until(what: &str, timeout: std::time::Duration, mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

/// Wait for `rhei run` to exit, rather than blocking forever if it does not.
#[cfg(unix)]
fn wait_for_exit(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => return status,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("rhei run did not exit within {timeout:?}");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}

/// The pid the fake agent recorded for itself, once it has recorded one.
#[cfg(unix)]
fn wait_for_recorded_pid(workspace: &Path, name: &str) -> String {
    let path = workspace.join("runtime/pids").join(name);
    poll_until(&format!("the fake agent to record its {name} pid"), TEST_PATIENCE, || {
        fs::read_to_string(&path).map(|text| !text.trim().is_empty()).unwrap_or(false)
    });
    fs::read_to_string(&path).expect("read recorded pid").trim().to_string()
}

/// The single agent transcript a one-ticket run produces.
#[cfg(unix)]
fn read_only_agent_log(workspace: &Path) -> String {
    let logs_dir = workspace.join("runtime/logs");
    let mut logs: Vec<PathBuf> = fs::read_dir(&logs_dir)
        .expect("agent log directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 1, "expected exactly one agent log, found {logs:?}");
    fs::read_to_string(&logs[0]).expect("read agent log")
}

#[cfg(unix)]
fn read_run_stderr(dir: &Path) -> String {
    fs::read_to_string(dir.join("run.err")).unwrap_or_default()
}

#[cfg(unix)]
fn read_run_stdout(dir: &Path) -> String {
    fs::read_to_string(dir.join("run.out")).unwrap_or_default()
}

/// Generous on purpose: every wait in these tests polls, so the only cost of a
/// large bound is how long a genuine failure takes to report.
#[cfg(unix)]
const TEST_PATIENCE: std::time::Duration = std::time::Duration::from_secs(30);

/// `SIGTERM` to `rhei run` must take the agent **and its grandchild** with it,
/// leave the ticket exactly where it was, and exit `128 + SIGTERM`.
///
/// This is issue #53: the supervisor died and its agent kept running,
/// reparented to init, still writing into the workspace with nobody left to
/// enforce its timeout or record its transition.
// §FS-rhei-run.3.2 §FS-rhei-agents.8
#[cfg(unix)]
#[test]
fn sigterm_to_the_run_ends_the_agent_and_its_grandchild() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-sigterm-group", GRANDCHILD_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");
    assert!(pid_is_alive(&agent), "the agent should be running");
    assert!(pid_is_alive(&grandchild), "the grandchild should be running");

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);

    // 128 + SIGTERM, the status a shell reports for a process SIGTERM killed.
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("the grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // The interruption is not a verdict on the ticket: it keeps its state and
    // the next run re-executes it.
    assert_task_state(&workspace, &machine_path, "1", "work");

    let log = read_only_agent_log(&workspace);
    assert!(
        log.contains("agent interrupted by run shutdown after"),
        "log should name the interruption, got:\n{log}"
    );
    assert!(log.contains("interrupted: true"), "log footer should flag it, got:\n{log}");
    assert!(!log.contains("timed_out: true"), "an interruption is not a timeout, got:\n{log}");

    let stderr = read_run_stderr(&dir);
    assert!(
        stderr.contains("Interrupted — terminating 1 invocation(s)"),
        "the shutdown notice should reach the operator, got:\n{stderr}"
    );

    // The run stopped; it did not complete, and it did not stop for human
    // attention. Both surfaces have to say the same thing.
    // §FS-rhei-run-report.3.1
    let stdout = read_run_stdout(&dir);
    assert!(
        !stdout.contains("Run complete:"),
        "an interrupted run must not claim completion, got:\n{stdout}"
    );
    let report = fs::read_to_string(workspace.join("runtime/run-report.md")).expect("run report");
    assert!(
        report.contains("Result: interrupted — re-run to continue"),
        "the report should name the interruption as the result, got:\n{report}"
    );
    assert!(
        report.contains("run interrupted while its worker was in state work"),
        "the Attention row should name the interruption as the blocker, got:\n{report}"
    );
    assert!(
        !report.contains("mark the task cancelled"),
        "an interrupted ticket is not something to cancel, got:\n{report}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// `SIGINT` — what a foreground Ctrl+C and the TUI's re-raise both deliver —
/// takes the same path and exits `130`.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
#[cfg(unix)]
#[test]
fn sigint_to_the_run_interrupts_it_and_exits_130() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-sigint-group", LONE_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let agent = wait_for_recorded_pid(&workspace, "agent");
    signal_pid(run.id(), "INT");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);

    assert_eq!(
        status.code(),
        Some(130),
        "run should exit 128+SIGINT\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    assert_task_state(&workspace, &machine_path, "1", "work");
    assert!(read_only_agent_log(&workspace).contains("interrupted: true"));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A supervisor `SIGKILL`ed runs no code at all, so nothing it installed can
/// tear its agents down. On Linux the agent's own parent-death signal does it.
///
/// `LONE_AGENT`, not `GRANDCHILD_AGENT`, on purpose: `PR_SET_PDEATHSIG` reaches
/// the direct subprocess and nothing below it. A grandchild of a `SIGKILL`ed
/// supervisor survives unless the agent tears it down as it dies, because
/// group-wide teardown needs the supervisor alive to signal the group.
/// Asserting a dead grandchild here would assert something the backstop does
/// not promise.
// §FS-rhei-run.3.2 §DA-supervised-process-groups
#[cfg(target_os = "linux")]
#[test]
fn sigkill_to_the_run_still_ends_the_agent() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-sigkill-pdeathsig", LONE_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let agent = wait_for_recorded_pid(&workspace, "agent");
    assert!(pid_is_alive(&agent), "the agent should be running");

    signal_pid(run.id(), "KILL");
    wait_for_exit(&mut run, TEST_PATIENCE);
    poll_until("the agent to die with its supervisor", TEST_PATIENCE, || !pid_is_alive(&agent));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A second interrupt means "now": the group is `SIGKILL`ed without waiting out
/// the grace.
///
/// The assertion is timing, and deliberately coarse. The agent ignores
/// `SIGTERM`, and this is a release-shaped binary, so its grace is the full
/// 10 s — a run that gets all the way out in a couple of seconds can only have
/// skipped it. The two signals are sent a beat apart because a second identical
/// signal delivered while the first is still pending would be coalesced into
/// one, and then there would be nothing to skip the grace.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_second_interrupt_skips_the_termination_grace() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-double-interrupt", STUBBORN_AGENT, "120s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let agent = wait_for_recorded_pid(&workspace, "agent");
    signal_pid(run.id(), "INT");
    // Long enough for the first signal to be delivered and handled, short
    // enough to be nowhere near the 10 s grace it is about to cut short.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let second = std::time::Instant::now();
    signal_pid(run.id(), "INT");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    let after_second = second.elapsed();
    assert_eq!(status.code(), Some(130), "stderr:\n{}", read_run_stderr(&dir));
    assert!(
        after_second < std::time::Duration::from_secs(6),
        "the second interrupt should skip the 10 s grace; the run took {after_second:?}\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));

    let stderr = read_run_stderr(&dir);
    assert!(
        stderr.contains("press Ctrl+C again to kill immediately"),
        "the notice should say a second signal is available, got:\n{stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A timeout signals the agent's **group**, so the MCP servers and shell tools
/// it started go with it. Before this, the timeout killed the direct child pid
/// and left the rest running.
// §FS-rhei-agents.7.3 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_timeout_ends_the_agents_whole_group() {
    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-timeout-group", GRANDCHILD_AGENT, "2s");
    let mut run = spawn_rhei_run(&dir, &workspace, &machine_path);

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert!(
        !status.success(),
        "a ticket whose agent timed out with no timeout transition cannot finish"
    );

    poll_until("the timed-out agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("its grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    let log = read_only_agent_log(&workspace);
    assert!(log.contains("agent timed out after"), "got:\n{log}");
    assert!(log.contains("timed_out: true"), "got:\n{log}");
    assert!(!log.contains("interrupted: true"), "a timeout is not an interruption, got:\n{log}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// An interrupted run must not start the work it had merely queued up.
///
/// Four tickets share one `concurrent: true` program state, so a single pass
/// collects all four and runs them one after another. A `SIGTERM` while the
/// first is in flight has to end the pass, not merely shorten each of the
/// remaining three to the moment its own `wait` reads the token — which is what
/// happened before the loop learned to check.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_interrupted_run_starts_none_of_the_programs_it_had_queued() {
    let plan = r#"# Rhei: Queued Programs

## Tasks

### Task 1: One
**State:** work

### Task 2: Two
**State:** work

### Task 3: Three
**State:** work

### Task 4: Four
**State:** work
"#;
    // `concurrent: true` is what lets one pass pick up all four tickets;
    // without it the state admits one at a time and there is nothing queued.
    let machine = r#"name: queued-programs
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: Sleep until told otherwise
    program: >-
      mkdir -p runtime/started
      && : > "runtime/started/$RHEI_TASK_ID"
      && sleep 300
  completed:
    description: Done
    final: true
transitions:
  - from: work
    to: completed
    exit_code: 0
"#;

    let dir = unique_temp_dir("run-interrupt-queued-programs");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    // `--parallel 1` runs programs one at a time from the pass's own loop,
    // which is the path this test is about.
    let mut run = spawn_rhei_run_with(&dir, &plan_path, &machine_path, &["--parallel", "1"]);

    let started_dir = dir.join("runtime/started");
    poll_until("the first program to start", TEST_PATIENCE, || {
        fs::read_dir(&started_dir).map(|entries| entries.count() >= 1).unwrap_or(false)
    });

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    let started: Vec<String> = fs::read_dir(&started_dir)
        .expect("started marker directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(started.len(), 1, "only the in-flight program may have run, got {started:?}");

    let mut logs: Vec<String> = fs::read_dir(dir.join("runtime/logs"))
        .expect("program log directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".log"))
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 1, "the shutdown should open no further program logs, got {logs:?}");

    // The three tickets that never ran are untouched, and so is the one that
    // did: an interruption is not a verdict. §FS-rhei-run.3.2
    for id in ["1", "2", "3", "4"] {
        assert_task_state(&plan_path, &machine_path, id, "work");
    }

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A run interrupted while a *program* is in flight must not answer by
/// spawning an agent.
///
/// The two are scheduled by separate loops in the same pass, and only the
/// program loop checked the token. A pass holding one ticket of each kind
/// therefore spent its whole shutdown inside the program loop and then fell
/// through to the sequential agent block with the run already stopping — and
/// started an agent there, under `bypassPermissions`, after the operator had
/// asked the run to stop.
// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_interrupted_run_starts_no_agent_after_its_sequential_program() {
    let plan = r#"# Rhei: Program Then Agent

## Tasks

### Task 1: Program
**State:** build

### Task 2: Agent
**State:** work
"#;

    let machine = r#"name: program-then-agent
version: 1
states:
  build:
    initial: true
    description: Sleep until told otherwise
    program: >-
      mkdir -p runtime/started
      && : > runtime/started/program
      && sleep 300
  work:
    description: Agent work
    agent: mock
    agent_timeout: 120s
  completed:
    description: Done
    final: true
transitions:
  - from: build
    to: completed
    exit_code: 0
  - from: work
    to: completed
"#;

    let agent = r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/started"
: > "$root/runtime/started/agent"
exec sleep 300
"#;

    let dir = unique_temp_dir("run-interrupt-program-then-agent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let agent_script = write_fixture_file(&dir, "mock-agent.sh", agent);
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "120s" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "timeout": "120s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    // `--parallel 1` is what puts the program on the pass's own loop and the
    // agent on the sequential block below it, which is the path under test.
    let mut run = spawn_rhei_run_with(&dir, &plan_path, &machine_path, &["--parallel", "1"]);

    let started_dir = dir.join("runtime/started");
    poll_until("the program to start", TEST_PATIENCE, || started_dir.join("program").exists());

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "run should exit 128+SIGTERM\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    assert!(
        !started_dir.join("agent").exists(),
        "an interrupted run must start no agent\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    // No subprocess, so no log and no journal entry either. §FS-rhei-run.3.2
    let logs: Vec<String> = fs::read_dir(dir.join("runtime/logs"))
        .expect("log directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("-work"))
        .collect();
    assert!(logs.is_empty(), "the shutdown should open no agent log, got {logs:?}");

    // Neither ticket moved: the program was interrupted and the agent never ran.
    assert_task_state(&plan_path, &machine_path, "1", "build");
    assert_task_state(&plan_path, &machine_path, "2", "work");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A `rhei run` driving a real TUI must end when it is signalled, not park on
/// its finished screen.
///
/// The engine joins the render thread before it writes the report and returns
/// its exit status, so a render thread that waits for `q` holds the whole
/// shutdown open: the run left no report, printed nothing, and ignored every
/// further signal. A pty is the only way to see it — the TUI is not selected
/// without one, and the `--no-tui` tests take a different path entirely.
// §FS-rhei-run-tui.1.5.7 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_external_signal_ends_a_tui_run_instead_of_parking_it() {
    use std::io::Read as _;
    use std::os::fd::OwnedFd;
    use std::sync::{Arc, Mutex};

    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-tui-sigterm", GRANDCHILD_AGENT, "120s");

    // A real size, or ratatui has no room to lay anything out; `openpty` with
    // no winsize leaves the terminal 0x0.
    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = nix::pty::openpty(Some(&winsize), None).expect("openpty");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", dir.join(".home"));
    cmd.env("TERM", "xterm-256color");
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        // `--tui` rather than relying on auto-detection, so a failure to reach
        // the TUI is a failure here and not a silent fallback to stdout.
        .arg("--tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard");
    // Both ends of the pty slave: crossterm reads keys from stdin, ratatui
    // draws to stdout, and the frontend picks the TUI from `stdout.is_terminal()`.
    let slave_in: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdin");
    let slave_out: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdout");
    cmd.stdin(std::process::Stdio::from(slave_in));
    cmd.stdout(std::process::Stdio::from(slave_out));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));
    // The child owns the slave now. Every copy left in this process has to go,
    // the `Command`'s own included — `spawn` keeps its `Stdio` handles until
    // the `Command` drops, and one surviving slave fd keeps the master
    // readable forever, hiding the child's exit from the drain thread below.
    drop(cmd);
    drop(pty.slave);

    // Drain the master continuously: a full pty buffer blocks the render
    // thread's writes, which would wedge the very shutdown under test.
    let screen: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let drained = Arc::clone(&screen);
    let mut master = std::fs::File::from(pty.master);
    let drain = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = master.read(&mut buf) {
            if n == 0 {
                break;
            }
            drained.lock().expect("screen buffer").extend_from_slice(&buf[..n]);
        }
    });
    let saw_alternate_screen = || {
        let seen = screen.lock().expect("screen buffer");
        seen.windows(8).any(|w| w == b"\x1b[?1049h")
    };

    poll_until("the TUI to enter the alternate screen", TEST_PATIENCE, saw_alternate_screen);
    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");

    signal_pid(run.id(), "TERM");
    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(143),
        "a signalled TUI run should exit 128+SIGTERM, not wait for `q`\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    join_or_detach(drain);

    poll_until("the agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("the grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // The engine got past the render-thread join and finished its own shutdown.
    let report = fs::read_to_string(workspace.join("runtime/run-report.md"))
        .expect("a signalled TUI run should still write its report");
    assert!(
        report.contains("Result: interrupted — re-run to continue"),
        "the report should name the interruption, got:\n{report}"
    );
    assert_task_state(&workspace, &machine_path, "1", "work");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Ctrl+C on the TUI's finished screen must leave the run its report.
///
/// The screen invites the key — the footer offers `^C` all run — but answering
/// it with `std::process::exit` from the render thread runs no destructor, and
/// the engine, blocked on joining that very thread, never reaches the report it
/// was about to write. The external-signal path was fixed and this one, which
/// the same screen invites, was not.
// §FS-rhei-run-tui.1.5.7 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn ctrl_c_on_the_finished_tui_screen_still_writes_the_report() {
    use std::io::{Read as _, Write as _};
    use std::os::fd::OwnedFd;
    use std::sync::{Arc, Mutex};

    let (dir, workspace, machine_path) =
        setup_supervised_workspace("run-tui-finished-ctrl-c", QUICK_AGENT, "120s");

    let winsize = nix::pty::Winsize { ws_row: 40, ws_col: 120, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = nix::pty::openpty(Some(&winsize), None).expect("openpty");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", dir.join(".home"));
    cmd.env("TERM", "xterm-256color");
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        .arg("--tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard");
    let slave_in: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdin");
    let slave_out: OwnedFd = pty.slave.try_clone().expect("clone pty slave for stdout");
    cmd.stdin(std::process::Stdio::from(slave_in));
    cmd.stdout(std::process::Stdio::from(slave_out));
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));
    drop(cmd);
    drop(pty.slave);

    let screen: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let drained = Arc::clone(&screen);
    let master = std::fs::File::from(pty.master);
    let mut writer = master.try_clone().expect("clone pty master for writing");
    let mut reader = master;
    let drain = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            drained.lock().expect("screen buffer").extend_from_slice(&buf[..n]);
        }
    });

    // The finished screen announces itself, which is the only reliable sign
    // that the render thread is parked in `stay_until_quit` rather than still
    // draining input from the live loop.
    poll_until("the TUI to park on its finished screen", TEST_PATIENCE, || {
        let seen = screen.lock().expect("screen buffer");
        seen.windows(9).any(|w| w == b"q to quit")
    });

    writer.write_all(b"\x03").expect("send Ctrl+C to the TUI");
    writer.flush().expect("flush Ctrl+C");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(130),
        "Ctrl+C should still exit 128+SIGINT\nstderr:\n{}",
        read_run_stderr(&dir)
    );
    join_or_detach(drain);

    // The point of the fix: the engine got past the join and wrote its report.
    let report = fs::read_to_string(workspace.join("runtime/run-report.md"))
        .expect("Ctrl+C on the finished screen should still leave a report");
    // The run had already finished when the key was pressed, so it reports the
    // result it reached — the interruption did not cut anything short.
    assert!(
        !report.contains("interrupted — re-run to continue"),
        "a run that finished before the key was pressed is not an interrupted run, got:\n{report}"
    );
    assert_task_state(&workspace, &machine_path, "1", "completed");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A fake agent that plays two parts, one per ticket. Ticket 1 waits for the
/// test's `go` marker and then exits `0`, so `rhei run` has something to print
/// at a moment the test chooses. Every other ticket backgrounds a grandchild
/// and sleeps, so a live process group is in flight when that print fails.
#[cfg(unix)]
const LOST_OUTPUT_AGENT: &str = r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
mkdir -p "$root/runtime/pids"
case "${RHEI_TASK_ID:?}" in
*1)
  : > "$root/runtime/pids/talker"
  n=0
  while [ ! -f "$root/runtime/go" ] && [ "$n" -lt 900 ]; do
    sleep 0.1
    n=$((n + 1))
  done
  exit 0
  ;;
*)
  sleep 300 &
  printf '%s\n' "$!" > "$root/runtime/pids/grandchild"
  printf '%s\n' "$$" > "$root/runtime/pids/agent"
  exec sleep 300
  ;;
esac
"#;

/// Losing the run's console output must not lose the run's subprocesses.
///
/// A `println!` to a pipe whose reader is gone panics, and the hook that turns
/// that into a quiet `141` leaves through `std::process::exit` — which runs no
/// destructor, so the shutdown guard never fires. Before this, the agent still
/// in flight was killed only by the Linux parent-death backstop and **its**
/// grandchild survived outright.
// §FS-rhei-run.3.2 §FS-rhei-run-tui.1.8
#[cfg(unix)]
#[test]
fn a_closed_stdout_still_ends_the_groups_in_flight() {
    // `concurrent: true` plus `--parallel 2` puts both tickets in flight at
    // once: one to make the run print, one to be left behind by the exit.
    let machine = r#"name: lost-output
version: 1
states:
  work:
    initial: true
    concurrent: true
    description: Do it
    agent: mock
    agent_timeout: 120s
  human-review:
    description: Wait for a human decision
    gating: true
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: human-review
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir("run-lost-stdout-groups");
    let workspace = dir.join("workspace");
    let tasks_dir = workspace.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(workspace.join("index.rhei.md"), "# Rhei: Lost Output\n").expect("write index");
    fs::write(tasks_dir.join("01-talker.md"), "### Task 1: Talker\n**State:** work\n")
        .expect("write task file");
    fs::write(tasks_dir.join("02-sleeper.md"), "### Task 2: Sleeper\n**State:** work\n")
        .expect("write task file");

    let agent_script = write_fixture_file(&dir, "mock-agent.sh", LOST_OUTPUT_AGENT);
    let settings_dir = workspace.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let script_json =
        serde_json::to_string(&agent_script.display().to_string()).expect("script path json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "120s" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "timeout": "120s" }} }}
}}"#
        ),
    )
    .expect("write settings");
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", dir.join(".home"));
    cmd.arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&workspace)
        .arg("--no-tui")
        .arg("--no-callbacks")
        .arg("--no-dashboard")
        .arg("--parallel")
        .arg("2");
    cmd.stdin(std::process::Stdio::null());
    // Piped, and deliberately never read: the run's output is far smaller than
    // a pipe buffer, so nothing blocks before the test closes the read end.
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(fs::File::create(dir.join("run.err")).expect("create run stderr"));
    let mut run = KillOnDrop(cmd.spawn().expect("rhei run should start"));

    let grandchild = wait_for_recorded_pid(&workspace, "grandchild");
    let agent = wait_for_recorded_pid(&workspace, "agent");
    poll_until("the talking agent to start", TEST_PATIENCE, || {
        workspace.join("runtime/pids/talker").exists()
    });

    // The reader is gone; the run's next `println!` has nowhere to go.
    drop(run.stdout.take().expect("piped stdout"));
    fs::write(workspace.join("runtime/go"), "").expect("release the talking agent");

    let status = wait_for_exit(&mut run, TEST_PATIENCE);
    assert_eq!(
        status.code(),
        Some(141),
        "a lost stdout should end the run the way a closed pipe ends a filter\nstderr:\n{}",
        read_run_stderr(&dir)
    );

    poll_until("the in-flight agent to be gone", TEST_PATIENCE, || !pid_is_alive(&agent));
    poll_until("its grandchild to be gone", TEST_PATIENCE, || !pid_is_alive(&grandchild));

    // Nothing transitioned the sleeper: it was terminated, not judged. The
    // talker did finish its state, which is what produced the failed print.
    assert_task_state(&workspace, &machine_path, "2", "work");

    fs::remove_dir_all(dir).expect("cleanup");
}
