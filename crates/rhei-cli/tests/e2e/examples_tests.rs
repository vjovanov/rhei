use std::fs;
use std::path::{Path, PathBuf};

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
    mkdir -p "runtime/worktrees/$task" runtime/worktree-refs
    {
      printf 'task_id: %s\n' "$task"
      printf 'path: %s\n' "$PWD/runtime/worktrees/$task"
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
    printf '# Mock worktree result\n\ntask=%s\nbranch=docs-pass/%s\n' "$task" "$task" \
      > "runtime/summaries/$task-result.md"
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
    let rhei_dir = workspace.join(".rhei");
    fs::create_dir_all(&rhei_dir).expect("create .rhei");
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
        rhei_dir.join("settings.json"),
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
    assert!(workspace.join("runtime/summaries/cli-result.md").exists());
    assert!(workspace.join("runtime/summaries/core-result.md").exists());
    assert!(workspace.join("runtime/summaries/validator-result.md").exists());

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
    assert!(workspace.join("runtime/reviews/task-spec-review-review-1.md").exists());
    assert!(workspace.join("runtime/reviews/task-spec-review-review-2.md").exists());
    assert!(workspace.join("runtime/fixes/task-spec-review-fix-1.md").exists());
    assert!(workspace.join("runtime/fixes/task-spec-review-fix-2.md").exists());

    fs::remove_dir_all(dir).expect("cleanup");
}
