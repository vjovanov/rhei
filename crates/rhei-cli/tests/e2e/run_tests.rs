use std::fs;

use super::*;

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

    for task_id in &["1", "2", "3"] {
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
        !result.stdout.contains("Task 1 transitioned: 'human-review' → 'completed'"),
        "gating task must not advance autonomously; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Task 2 transitioned: 'work' → 'completed'"),
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

    let template_path = repo_root.join(".agents/rhei/templates/changeset-review/states.yaml");
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

    let reset_result = run_cli("reset", &workspace_path, &machine_path, &[]);
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
