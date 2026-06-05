fn write_run_agent_settings(dir: &Path, settings: &str) {
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(settings_dir.join("settings.json"), settings).expect("write settings");
}

fn canonical_path_text(path: &Path) -> String {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

fn recorded_value<'a>(recorded: &'a str, prefix: &str) -> &'a str {
    recorded
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("recorded output missing {prefix:?}: {recorded}"))
}

fn assert_recorded_path_eq(recorded: &str, expected: &Path) {
    assert_eq!(canonical_path_text(Path::new(recorded)), canonical_path_text(expected));
}

fn make_run_agent_script_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("stat agent script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod agent script");
    }
    #[cfg(not(unix))]
    let _ = path;
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

const CHECKOUT_ROOT_MACHINE: &str = r#"name: checkout-root-agent
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

const CHECKOUT_ROOT_PLAN: &str = r#"# Rhei: Checkout Root

## Tasks

### Task 1: Record checkout root
**State:** review
"#;

fn write_checkout_recording_script(dir: &Path) -> PathBuf {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$RHEI_ROOT/runtime"
{
  pwd
  printf 'rhei=%s\n' "$RHEI_ROOT"
  printf 'checkout=%s\n' "$RHEI_CHECKOUT_ROOT"
  printf 'worktree=%s\n' "${RHEI_WORKTREE_ROOT:-}"
} > "$RHEI_ROOT/runtime/checkout-root.txt"
"#;
    let script_path = write_fixture_file(dir, "record-checkout.sh", script);
    make_run_agent_script_executable(&script_path);
    script_path
}

fn write_absolute_fake_agent_settings(dir: &Path, script_path: &Path) {
    let settings = format!(
        r#"{{
  "agents": {{
    "fake": {{
      "command": [{}],
      "timeout": "5s"
    }}
  }}
}}"#,
        serde_json::to_string(&script_path.display().to_string()).expect("script path json")
    );
    write_run_agent_settings(dir, &settings);
}

fn write_stdin_fake_agent_settings(dir: &Path, script_path: &Path) {
    let settings = format!(
        r#"{{
  "agents": {{
    "fake": {{
      "command": [{}],
      "stdin_prompt": true,
      "timeout": "5s"
    }}
  }}
}}"#,
        serde_json::to_string(&script_path.display().to_string()).expect("script path json")
    );
    write_run_agent_settings(dir, &settings);
}

fn run_git(args: &[&str]) {
    let output = Command::new("git").args(args).output().expect("git should run");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_git_repo(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo");
    run_git(&["-C", repo.to_str().expect("repo path"), "init"]);
    run_git(&["-C", repo.to_str().expect("repo path"), "config", "user.email", "rhei@example.test"]);
    run_git(&["-C", repo.to_str().expect("repo path"), "config", "user.name", "Rhei Test"]);
    fs::write(repo.join("README.md"), "repo\n").expect("write readme");
    run_git(&["-C", repo.to_str().expect("repo path"), "add", "README.md"]);
    run_git(&["-C", repo.to_str().expect("repo path"), "commit", "-m", "initial"]);
}

#[test]
fn run_agent_uses_enclosing_git_root_as_checkout_root() {
    let root = unique_temp_dir("run-agent-git-checkout-root");
    let repo = root.join("repo");
    init_git_repo(&repo);
    fs::write(repo.join("AGENTS.md"), "root instructions\n").expect("write agents");

    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script_path = write_checkout_recording_script(&plan_dir);
    write_absolute_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: repository-root checkout context lets agents discover root AGENTS.md.
    assert_recorded_path_eq(recorded.lines().next().expect("recorded cwd"), &repo);
    assert_recorded_path_eq(recorded_value(&recorded, "rhei="), &plan_dir);
    assert_recorded_path_eq(recorded_value(&recorded, "checkout="), &repo);
    assert!(plan_dir.join("runtime/logs/task-1-review.log").exists());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_agent_renders_artifact_paths_at_rhei_root_when_checkout_root_differs() {
    let root = unique_temp_dir("run-agent-artifact-path-checkout-root");
    let repo = root.join("repo");
    init_git_repo(&repo);

    let machine = r#"name: checkout-artifact-root
version: 1
states:
  review:
    initial: true
    agent: fake
    instructions: "ARTIFACT={output.report.path}"
    outputs:
      - name: report
        path: runtime/reports/{task_id}.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#;
    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", machine);
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
prompt="$(cat)"
path="$(printf '%s\n' "$prompt" | sed -n 's/^ARTIFACT=//p' | head -n 1)"
mkdir -p "$(dirname "$path")"
printf done > "$path"
mkdir -p "$RHEI_ROOT/runtime"
printf '%s\n' "$path" > "$RHEI_ROOT/runtime/rendered-artifact-path.txt"
"#;
    let script_path = write_fixture_file(&plan_dir, "write-rendered-artifact.sh", script);
    make_run_agent_script_executable(&script_path);
    write_stdin_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed when the agent writes the rendered artifact path\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let expected = plan_dir.join("runtime/reports/1.md");
    assert!(expected.exists(), "required output should be written under RHEI_ROOT");
    let rendered = fs::read_to_string(plan_dir.join("runtime/rendered-artifact-path.txt"))
        .expect("read rendered path");
    // §FS-rhei-agents.4: artifact template paths stay rooted at RHEI_ROOT when cwd is checkout root.
    assert_recorded_path_eq(rendered.trim(), &expected);
    assert!(
        !repo.join("runtime/reports/1.md").exists(),
        "agent should not write checkout-root runtime artifacts"
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_agent_falls_back_to_invocation_cwd_when_no_git_root_exists() {
    let root = unique_temp_dir("run-agent-cwd-checkout-root");
    let cwd = root.join("caller");
    let plan_dir = root.join("scratchpad");
    fs::create_dir_all(&cwd).expect("create cwd");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script_path = write_checkout_recording_script(&plan_dir);
    write_absolute_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command_in_dir(&cwd, &plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: no-git runs use the operator's invocation cwd as checkout context.
    assert_recorded_path_eq(recorded.lines().next().expect("recorded cwd"), &cwd);
    assert_recorded_path_eq(recorded_value(&recorded, "rhei="), &plan_dir);
    assert_recorded_path_eq(recorded_value(&recorded, "checkout="), &cwd);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_agent_clears_inherited_worktree_env_without_task_worktree_ref() {
    let root = unique_temp_dir("run-agent-clear-stale-worktree-env");
    let plan_dir = root.join("scratchpad");
    fs::create_dir_all(&plan_dir).expect("create plan dir");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    let script_path = write_checkout_recording_script(&plan_dir);
    write_absolute_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command_with_env(
        &plan_path,
        &machine_path,
        &["--no-callbacks"],
        &[("RHEI_WORKTREE_ROOT", "/stale/worktree")],
    );

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: RHEI_WORKTREE_ROOT is unset unless a task worktree ref applies.
    assert!(recorded.contains("worktree=\n"), "{recorded}");
    assert!(!recorded.contains("/stale/worktree"), "{recorded}");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_agent_prefers_task_worktree_ref_over_repository_root() {
    let root = unique_temp_dir("run-agent-task-worktree-root");
    let repo = root.join("repo");
    init_git_repo(&repo);
    let worktree = root.join("worktrees/task-1");
    let worktree_parent = worktree.parent().expect("worktree parent");
    fs::create_dir_all(worktree_parent).expect("create worktree parent");
    run_git(&[
        "-C",
        repo.to_str().expect("repo path"),
        "worktree",
        "add",
        "-b",
        "rhei/task-1",
        worktree.to_str().expect("worktree path"),
    ]);

    let plan_dir = repo.join(".agents/scratchpad/review");
    fs::create_dir_all(plan_dir.join("runtime/worktree-refs")).expect("create worktree refs");
    let plan_path = write_fixture_file(&plan_dir, "plan.rhei.md", CHECKOUT_ROOT_PLAN);
    let machine_path = write_fixture_file(&plan_dir, "states.yaml", CHECKOUT_ROOT_MACHINE);
    fs::write(
        plan_dir.join("runtime/worktree-refs/1.yaml"),
        format!("path: {}\nbranch: rhei/task-1\n", worktree.display()),
    )
    .expect("write worktree ref");
    let script_path = write_checkout_recording_script(&plan_dir);
    write_absolute_fake_agent_settings(&plan_dir, &script_path);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let recorded =
        fs::read_to_string(plan_dir.join("runtime/checkout-root.txt")).expect("read checkout log");
    // §FS-rhei-agents.4: per-task worktree refs override the enclosing repository root.
    assert_recorded_path_eq(recorded.lines().next().expect("recorded cwd"), &worktree);
    assert_recorded_path_eq(recorded_value(&recorded, "checkout="), &worktree);
    assert_recorded_path_eq(recorded_value(&recorded, "worktree="), &worktree);

    fs::remove_dir_all(root).expect("cleanup");
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
fn run_auto_advances_nested_agent_task_after_outputs_exist() {
    let machine = r#"name: nested-agent-output
version: 1
states:
  waiting:
    gating: true
  pending:
    initial: true
    agent: fake
    outputs:
      - name: report
        path: runtime/reports/{task_id}.md
  completed:
    final: true
transitions:
  - from: pending
    to: completed
"#;
    let plan = r#"# Rhei: Nested Agent Output

## Tasks

### Task 1: Parent
**State:** waiting

#### Task 1.1: Child agent work
**State:** pending
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
mkdir -p runtime/reports
printf done > runtime/reports/1.1.md
"#;

    let dir = unique_temp_dir("run-nested-agent-output");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);
    let script_path = write_fixture_file(&dir, "agent.sh", script);
    make_run_agent_script_executable(&script_path);
    write_run_agent_settings(&dir, settings);

    let result = run_run_command(&plan_path, &machine_path, &["--no-callbacks"]);

    assert!(
        result.status.success(),
        "run should spawn and auto-advance the nested agent task\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(dir.join("runtime/reports/1.1.md").exists(), "agent should write required output");
    // §FS-rhei-run.3: Successful agent output on a child task still applies the selected transition.
    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse plan");
    assert_eq!(rhei.tasks[0].state.as_str(), "waiting");
    assert_eq!(rhei.tasks[0].children[0].state.as_str(), "completed");

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
fn run_program_exit_zero_missing_outputs_fails_run_and_leaves_task_in_place() {
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
        !result.status.success(),
        "run should fail when a successful program misses outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("program exited 0 but required outputs are missing"),
        "missing-output warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("non-terminal tasks remaining"),
        "run should report stalled non-terminal work; got stdout:\n{}\nstderr:\n{}",
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
fn run_agent_exit_zero_missing_outputs_emits_failure_snapshots_and_fails_run() {
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
        !result.status.success(),
        "run should fail on missing outputs\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("agent exited 0 but required outputs are missing"),
        "missing-output warning should be shown; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("non-terminal tasks remaining"),
        "run should report stalled non-terminal work; got stdout:\n{}\nstderr:\n{}",
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
