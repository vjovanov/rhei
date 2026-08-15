use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

fn copy_example_workspace(prefix: &str, example_path: &str) -> (PathBuf, PathBuf) {
    let dir = unique_scratchpad_dir(prefix);
    let src = repo_root().join(example_path);
    let leaf = Path::new(example_path).file_name().expect("example path has leaf");
    let workspace = dir.join(leaf);
    copy_dir_recursive(&src, &workspace);
    (dir, workspace)
}

fn write_mock_example_agent(dir: &Path) -> String {
    let script = dir.join("mock-example-agent.sh");
    fs::write(
        &script,
        r#"#!/bin/sh
set -eu

workspace="${RHEI_PLAN_PATH:-.}"
if [ -f "$workspace" ]; then
  workspace="$(dirname "$workspace")"
fi
cd "$workspace"

state="${RHEI_STATE:-}"
task="${RHEI_TASK_ID:-unknown}"
target_slug="${RHEI_TARGET_SLUG:-${RHEI_MODEL:-mock}}"
machine="${RHEI_STATE_MACHINE_PATH:-}"

mkdir -p runtime/logs
printf 'task=%s state=%s model=%s target=%s agent=%s\n' \
  "$task" "$state" "${RHEI_MODEL:-}" "$target_slug" "${RHEI_AGENT:-}" \
  >> runtime/logs/mock-agent.log

# A worker records why the ticket ends where it does. Rhei hands the path to
# every subprocess in RHEI_RESULT_PATH, and a `final: true` state is not entered
# until it has content. §FS-rhei-states.3.3 §FS-rhei-agents.4
mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
printf '## Result\n\nMock agent finished state %s for task %s.\n' "$state" "$task" \
  > "$RHEI_RESULT_PATH"

case "$state" in
  analyze)
    if [ -n "$machine" ] && grep -q '^name: multi-model-analysis' "$machine"; then
      mkdir -p runtime/analyses
      printf '# Mock analysis\n\nstate=%s\ntarget=%s\n' "$state" "$target_slug" \
        > "runtime/analyses/$target_slug.md"
    else
      mkdir -p runtime/analysis tasks
      printf '# Mock dispatch findings\n\n- id: mock-work\n  title: Mock work item\n' \
        > "runtime/analysis/$task-findings.md"
      if [ ! -f tasks/02-mock-work.md ]; then
        cat > tasks/02-mock-work.md <<EOF
### Task mock-work: Mock dispatched work item
**State:** address
**Prior:** Task $task

Write the mock work result.
EOF
      fi
      if [ ! -f tasks/03-report.md ]; then
        cat > tasks/03-report.md <<'EOF'
### Task report: Summarize the dispatched work
**State:** report
**Prior:** Task mock-work

Summarize the mock work result.
EOF
      fi
    fi
    ;;
  address)
    mkdir -p runtime/work
    printf '# Mock work result\n\ntask=%s\n' "$task" > "runtime/work/$task.md"
    ;;
  report)
    mkdir -p runtime
    printf '# Mock dispatch report\n' > runtime/report.md
    ;;
  prepare-worktree)
    worktree_path="$PWD/runtime/worktrees/$task"
    mkdir -p "$(dirname "$worktree_path")" runtime/worktree-refs
    if git -C "${RHEI_CHECKOUT_ROOT:-.}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      rm -rf "$worktree_path"
      git -C "${RHEI_CHECKOUT_ROOT:-.}" worktree prune >/dev/null 2>&1 || true
      git -C "${RHEI_CHECKOUT_ROOT:-.}" worktree add --detach "$worktree_path" HEAD >/dev/null
    else
      mkdir -p "$worktree_path"
    fi
    {
      printf 'task_id: %s\n' "$task"
      printf 'path: %s\n' "$worktree_path"
      printf 'branch: docs-pass/%s\n' "$task"
      printf 'target_path: mock\n'
    } > "runtime/worktree-refs/$task.yaml"
    ;;
  work)
    mkdir -p runtime/summaries
    printf '# Mock worktree change summary\n\ntask=%s\n' "$task" \
      > "runtime/summaries/$task-work.md"
    ;;
  integrate)
    mkdir -p runtime/summaries
    printf '# Mock worktree summary\n\ntask=%s\nbranch=docs-pass/%s\n' "$task" "$task" \
      > "runtime/summaries/$task-summary.md"
    ;;
  summarize)
    mkdir -p runtime
    printf '# Mock final analysis\n' > runtime/final-analysis.md
    ;;
  review)
    mkdir -p runtime/reviews
    n="$(find runtime/reviews -maxdepth 1 -name "task-$task-review-*.md" 2>/dev/null | wc -l | tr -d ' ')"
    n=$((n + 1))
    printf '# Mock review pass %s\n' "$n" > "runtime/reviews/task-$task-review-$n.md"
    ;;
  fix)
    mkdir -p runtime/fixes
    n="$(find runtime/fixes -maxdepth 1 -name "task-$task-fix-*.md" 2>/dev/null | wc -l | tr -d ' ')"
    n=$((n + 1))
    printf '# Mock fix pass %s\n' "$n" > "runtime/fixes/task-$task-fix-$n.md"
    ;;
  collect|judge|apply)
    ;;
esac
"#,
    )
    .expect("write mock example agent");
    script.display().to_string()
}

fn write_mock_agent_settings(workspace: &Path, agent_script: &str) {
    let settings_dir = workspace.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agents/rhei");
    let profile = format!(
        r#"{{
      "command": ["sh", {}],
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}"#,
        serde_json::to_string(agent_script).expect("json string")
    );
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{
    "agent": "mock",
    "agent_timeout": "5s"
  }},
  "agents": {{
    "mock": {profile},
    "claude-code": {profile},
    "codex": {profile},
    "gemini": {profile},
    "cursor": {profile}
  }},
  "models": {{
    "claude": {{ "provider": "mock", "model": "claude", "default_agent": "mock" }},
    "codex": {{ "provider": "mock", "model": "codex", "default_agent": "mock" }},
    "gemini": {{ "provider": "mock", "model": "gemini", "default_agent": "mock" }},
    "cursor": {{ "provider": "mock", "model": "cursor", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write mock settings");
}

fn run_example_with_mock_agents(
    prefix: &str,
    example_path: &str,
    state_machine_name: &str,
    args: &[&str],
) -> (PathBuf, PathBuf, PathBuf, CliRun) {
    let (dir, workspace) = copy_example_workspace(prefix, example_path);
    let agent = write_mock_example_agent(&dir);
    write_mock_agent_settings(&workspace, &agent);
    let machine_path = workspace.join(state_machine_name);
    let result = run_cli("run", &workspace, &machine_path, args);
    (dir, workspace, machine_path, result)
}

#[test]
fn example_agent_discussion_runs_with_mock_agents() {
    let (dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-agent-discussion",
        "examples/agent-discussion",
        "discussion-states.yaml",
        &["--no-tui"],
    );
    assert_success(&result);

    let json = render_json(&workspace, &machine_path);
    let states: Vec<&str> = json["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|task| task["state"].as_str().expect("state field"))
        .collect();
    assert!(
        states.contains(&"converged") && states.contains(&"completed"),
        "expected discussion seed to converge and downstream task to complete; got:\n{}",
        result.stdout
    );
    assert!(workspace.join("runtime/discussion/decision.md").exists());
    assert!(workspace.join("runtime/discussion/applied.md").exists());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn example_analyze_and_dispatch_runs_with_mock_agents() {
    let (dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-analyze-dispatch",
        "examples/analyze-and-dispatch-example",
        "states.yaml",
        &["--no-tui", "--parallel", "3"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    assert!(workspace.join("tasks/02-mock-work.md").exists());
    assert!(workspace.join("runtime/report.md").exists());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn example_parallel_worktrees_runs_with_mock_agents() {
    let (dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-parallel-worktrees",
        "examples/parallel-worktrees-example",
        "states.yaml",
        &["--no-tui", "--parallel", "3"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    // The integrate state's artifact is `summary`, not `result`: a declared
    // artifact called `result` would collide in the prompt and in stall reports
    // with the ticket's own terminal result. §FS-rhei-states.3.3
    assert!(workspace.join("runtime/summaries/parallel-worktrees-example.cli-summary.md").exists());
    assert!(workspace
        .join("runtime/summaries/parallel-worktrees-example.core-summary.md")
        .exists());
    assert!(workspace
        .join("runtime/summaries/parallel-worktrees-example.validator-summary.md")
        .exists());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn example_multi_model_analysis_runs_with_mock_agents() {
    let (dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-multi-model-analysis",
        "examples/multi-model-analysis-example",
        "states.yaml",
        &["--no-tui"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    assert!(workspace.join("runtime/final-analysis.md").exists());
    assert!(workspace
        .join("runtime/analyses/claude-code-yolo-anthropic-claude-opus-4-7.md")
        .exists());
    assert!(workspace
        .join("runtime/analyses/gemini-yolo-google-gemini-3.1-pro-preview.md")
        .exists());
    assert!(workspace.join("runtime/analyses/codex-yolo-openai-gpt-5-codex.md").exists());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn example_spec_review_runs_with_mock_agents() {
    let (dir, workspace, machine_path, result) = run_example_with_mock_agents(
        "example-spec-review",
        "examples/spec-review-example",
        "states.yaml",
        &["--no-tui"],
    );
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine_path, "completed");
    assert!(workspace.join("specs/template-review-fixture.spec.md").exists());
    assert!(workspace.join("runtime/reviews/task-spec-review-example.review-review-1.md").exists());
    assert!(workspace.join("runtime/reviews/task-spec-review-example.review-review-2.md").exists());
    assert!(workspace.join("runtime/fixes/task-spec-review-example.review-fix-1.md").exists());
    assert!(workspace.join("runtime/fixes/task-spec-review-example.review-fix-2.md").exists());

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The bundled UI fixture, instantiated and run end to end with its own mock
/// agent and program.
///
/// This is the one bundled workspace whose workers are committed scripts rather
/// than a real agent, so it is the one that breaks silently when the engine
/// starts asking workers for something. It did: `script-check -> completed` is a
/// program state whose exit-0 edge is terminal, and neither mock wrote
/// `RHEI_RESULT_PATH`, so every terminal ticket stalled on a missing `result`.
/// Instantiating and running here keeps the fixture honest about the contract
/// it demonstrates.
// §FS-rhei-states.3.3 §FS-rhei-agents.4 §FS-rhei-programs.2
#[test]
fn bundled_ui_fixture_instantiates_and_runs_to_its_human_gate() {
    let dir = unique_temp_dir("example-ui-test-canonical");
    let template = dir.join(".agents/rhei/templates/ui-test-canonical");
    copy_dir_recursive(&repo_root().join(".agents/rhei/templates/ui-test-canonical"), &template);
    let home = dir.join(".home");
    fs::create_dir_all(&home).expect("isolated home");

    let instantiate = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["instantiate", "ui-test-canonical", "--output", "ws"])
        .output()
        .expect("rhei instantiate should run");
    assert!(
        instantiate.status.success(),
        "instantiate failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&instantiate.stdout),
        String::from_utf8_lossy(&instantiate.stderr)
    );

    let run = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["run", "ws", "--no-tui", "--parallel", "4"])
        .output()
        .expect("rhei run should run");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(
        run.status.success(),
        "the fixture must run to its human gate:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("required outputs are missing"),
        "no worker may stall on a missing artifact:\nstdout:\n{stdout}"
    );

    let workspace = dir.join("ws");
    // A program-driven terminal edge and an agent-driven one, each with the
    // worker's own account on disk.
    for task in
        ["ws.scenario-dashboard-checkout-flow", "ws.full-pipeline.snapshot-inherit-ancestor"]
    {
        let result = workspace.join(format!("runtime/results/{task}.md"));
        let body = fs::read_to_string(&result)
            .unwrap_or_else(|err| panic!("read {}: {err}", result.display()));
        assert!(!body.trim().is_empty(), "{task}: terminal result must have content");
    }

    // Every invocation gets its own fragment, keyed by the state and visit it
    // belongs to, so no reviewer overwrites another and no later state inherits
    // this one's account. §FS-rhei-states.3.3
    let fragments = workspace.join("runtime/results/ws.full-pipeline/parallel-review/1");
    let mut names: Vec<String> = fs::read_dir(&fragments)
        .unwrap_or_else(|err| panic!("fan-out result fragments at {}: {err}", fragments.display()))
        .map(|entry| entry.expect("fragment entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "one fragment per review target; got {names:?}");

    fs::remove_dir_all(dir).expect("cleanup");
}
