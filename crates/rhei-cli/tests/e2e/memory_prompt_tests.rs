// §FS-rhei-memory driven end to end: the four sections as a spawned agent
// actually receives them, and the same sections as `rhei next` hands them to a
// manual worker.

use std::fs;

use super::supervision_tests::{prompt_for, setup_supervision};
use super::*;

/// A plain two-state workflow: enough for a ticket to be worked, reviewed, and
/// finished, so a later ticket has a history to read.
const MEMORY_MACHINE: &str = r#"name: memory-e2e
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    agent: mock
    agent_timeout: 30s
    instructions: Do the work for Task {task_id}.
  review:
    description: Review
    agent: mock
    agent_timeout: 30s
    instructions: Review Task {task_id}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: review, description: Work done }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// Two root tickets, the second waiting on the first for both a prior and an
/// export, plus a plan-level standing note.
const MEMORY_PLAN: &str = r#"# Rhei: Memory

## House Rules

Write the one-line summary first.

## Tasks

### Task 1: Build the index
**State:** pending
**Provides:** findings

### Task 2: Query the index
**State:** pending
**Prior:** 1
**Consumes:** 1:findings
"#;

/// §FS-rhei-memory.3: every section a spawned agent is owed reaches it, at the
/// position the spec puts it, with the ids qualified throughout.
#[test]
fn a_spawned_agent_receives_its_position_history_and_map() {
    let (dir, plan_path, machine_path) =
        setup_supervision("memory-run", MEMORY_PLAN, MEMORY_MACHINE, "");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    // §FS-rhei-memory.3.1: orientation comes before the instructions.
    let first = prompt_for(&dir, "plan.1", "pending", 1);
    assert!(
        first.contains(
            "## Position\n\nPanta: Memory \u{203a} rhei `plan`: Memory\n\
             \u{203a} **Task plan.1: Build the index [pending]** \u{2190} this invocation \
             (visit 1)\n"
        ),
        "got:\n{first}"
    );
    assert!(
        first.find("## Position") < first.find("## Instructions"),
        "orientation precedes the instructions; got:\n{first}"
    );
    // §FS-rhei-memory.3.1: the plan writer's standing notes, which until now
    // only a worker that opened the file read.
    assert!(first.contains("### Rhei Context"), "got:\n{first}");
    assert!(first.contains("Write the one-line summary first."), "got:\n{first}");
    assert!(!first.contains("### Project Context"), "a bare rhei has none; got:\n{first}");
    // §FS-rhei-memory.3.2: who reads what this task writes.
    assert!(
        first.contains(
            "### Dependents\n\n- Task plan.2: Query the index [pending] \u{2014} prior, \
             consumes `findings`\n"
        ),
        "got:\n{first}"
    );
    // §FS-rhei-memory.3.3: nothing has happened to it yet.
    assert!(!first.contains("## Previous Visits"), "got:\n{first}");
    // §FS-rhei-memory.3.4: the map, after the transition list. The fixture is
    // not a git checkout, so the agent's cwd is elsewhere and every path is
    // absolute — the rule `{output.<name>.path}` follows.
    let root = dir.display().to_string();
    let dash = "\u{2014}";
    assert!(
        first.contains(&format!(
            "- This rhei: `{root}` {dash} plan `{root}/plan.rhei.md`, \
             this task's file `{root}/plan.rhei.md`\n"
        )),
        "got:\n{first}"
    );
    assert!(first.contains(&format!("  - `plan` {dash} `{root}`\n")), "got:\n{first}");
    assert!(
        first.find("Available transitions from") < first.find("### Reading the rhei"),
        "the map follows the transition list; got:\n{first}"
    );
    assert!(first.contains("### Leaving a trail"), "got:\n{first}");
    // §FS-rhei-memory.3.4: under `rhei run` the two sub-sections already have a
    // `##` parent — `## Rhei Commands` — and get no second one.
    assert!(!first.contains("## Rhei Navigation"), "got:\n{first}");

    // §FS-rhei-memory.3.3: the second state of the same ticket knows what
    // already happened to it.
    let reviewed = prompt_for(&dir, "plan.1", "review", 1);
    // §FS-rhei-memory.4.4: the ledger holds `pending@review`, so it already
    // ends in the state being entered — that state is annotated, not repeated.
    assert!(
        reviewed.contains("Trail for this task: pending \u{2192} review (this visit, visit 1).\n"),
        "got:\n{reviewed}"
    );
    assert!(reviewed.contains("Task plan.1 finished pending."), "got:\n{reviewed}");

    // §FS-rhei-memory.3.2: the next ticket reads what finished before it, and
    // pays for the prior's result once.
    let second = prompt_for(&dir, "plan.2", "pending", 1);
    assert!(
        second.contains("- Task plan.1: Build the index \u{2014} completed \u{2014} see above\n"),
        "the prior is pasted in full above, so the history refers to it; got:\n{second}"
    );
    assert!(second.contains("## Prior Task Results"), "got:\n{second}");
    // §FS-rhei-memory.4.5: and that pasted body is fenced.
    assert!(second.contains("### Task plan.1\n\n```markdown\n## Result"), "got:\n{second}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-memory.5: `rhei next` renders the same sections from the same
/// renderers — after the instructions in text, one string field each in JSON.
#[test]
fn rhei_next_mirrors_the_memory_sections_in_text_and_json() {
    let (dir, plan_path, machine_path) =
        setup_supervision("memory-next", MEMORY_PLAN, MEMORY_MACHINE, "");
    // A finished first ticket and a ledger to place it in time.
    fs::create_dir_all(dir.join("runtime/results")).expect("runtime tree");
    write_fixture_file(&dir, "runtime/state-transitions.log", "plan.1 review@completed\n");
    write_fixture_file(
        &dir,
        "runtime/results/plan.1.md",
        "## Result\n\nIndex built over 12k documents.\n",
    );
    let plan = fs::read_to_string(&plan_path).expect("read plan");
    fs::write(
        &plan_path,
        plan.replacen(
            "**State:** pending\n**Provides:**",
            "**State:** completed\n**Provides:**",
            1,
        ),
    )
    .expect("finish task 1");

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "2", "--peek"]);
    assert_success(&peek);
    let instructions = peek.stdout.find("--- Instructions").expect("instructions");
    for section in ["\n## Position\n", "\n## Plan History\n", "\n### Reading the rhei\n"] {
        let at =
            peek.stdout.find(section).unwrap_or_else(|| panic!("{section} in:\n{}", peek.stdout));
        assert!(at > instructions, "{section} follows the instructions; got:\n{}", peek.stdout);
    }
    // §FS-rhei-memory.5: `rhei next` prints no `## Rhei Commands`, so the two
    // sub-sections get a `##` parent of their own on this surface.
    assert!(
        peek.stdout.contains("\n## Rhei Navigation\n\n### Reading the rhei\n"),
        "got:\n{}",
        peek.stdout
    );
    assert!(
        peek.stdout.contains(
            "- Task plan.1: Build the index \u{2014} completed \u{2014} Index built over 12k \
             documents.\n"
        ),
        "got:\n{}",
        peek.stdout
    );

    let json = run_cli("next", &plan_path, &machine_path, &["--task", "2", "--peek", "--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("next --json parses");
    assert!(
        payload["position"].as_str().expect("position").contains("**Task plan.2:"),
        "got: {payload}"
    );
    assert!(
        payload["plan_history"].as_str().expect("plan_history").contains("Index built over"),
        "got: {payload}"
    );
    assert!(
        payload["navigation"].as_str().expect("navigation").contains("### Leaving a trail"),
        "got: {payload}"
    );
    assert!(
        payload["navigation"].as_str().expect("navigation").starts_with("## Rhei Navigation"),
        "the field carries the section as rendered; got: {payload}"
    );
    // Task 2 has never been visited, so the section — and its field — is absent.
    assert!(payload.get("previous_visits").is_none(), "got: {payload}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-memory.3.4: `rhei next` exports no `RHEI_ROOT` and promises the
/// reader no working directory, so every path it prints is absolute — and in a
/// project of several rheis, each root is its own directory, not `.` twice.
#[test]
fn rhei_next_renders_every_memory_path_absolute() {
    let dir = unique_temp_dir("memory-next-paths");
    write_fixture_file(
        &dir,
        "index.panta.md",
        "# Panta: Map\n\n## House Rules\n\nRun the tests.\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", MEMORY_MACHINE);
    for rhei in ["alpha", "beta"] {
        fs::create_dir_all(dir.join(rhei).join("tasks")).expect("workspace dirs");
        fs::write(
            dir.join(rhei).join("index.rhei.md"),
            format!("# Rhei: {rhei}\n\n## Ground Rules\n\nKeep {rhei} stable.\n"),
        )
        .expect("write index");
        fs::write(
            dir.join(rhei).join("tasks/work.md"),
            format!("### Task 1: Work {rhei}\n**State:** pending\n"),
        )
        .expect("write task file");
    }
    write_fixture_file(
        &dir,
        "gamma.rhei.md",
        "# Rhei: Gamma\n\n## Tasks\n\n### Task 1: Work gamma\n**State:** pending\n",
    );

    // A cwd that is not the project directory: a relative path would resolve
    // against this, and the reader has no way to know that.
    let mut cmd = rhei_command(dir.join(".home"));
    cmd.current_dir(repo_root());
    cmd.arg("--state-machine").arg(&machine_path).arg("next").arg(&dir);
    cmd.args(["--task", "alpha.1", "--peek"]);
    let output = cmd.output().expect("rhei next should run");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "next should succeed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let map = stdout.split("### Reading the rhei").nth(1).expect("the map is printed");
    // The bullets after the map are literal artifact templates, not paths.
    let map = map.split("- Under each execution root:").next().expect("the map ends");
    let quoted: Vec<&str> = map.split('`').skip(1).step_by(2).collect();
    let paths: Vec<&str> = quoted.iter().copied().filter(|token| token.contains('/')).collect();
    assert!(paths.len() >= 6, "the map names every rhei's root; got:\n{map}");
    for path in &paths {
        assert!(Path::new(path).is_absolute(), "`{path}` is not absolute; got:\n{map}");
        assert!(Path::new(path).exists(), "`{path}` does not exist; got:\n{map}");
    }

    // §FS-rhei-memory.1.1: three rheis, three roots — the map is only a map
    // while no two rheis answer to the same string.
    let roots: Vec<&str> = map
        .lines()
        .filter(|line| line.starts_with("  - `"))
        .filter_map(|line| line.split('`').nth(3))
        .collect();
    assert_eq!(roots.len(), 3, "one line per rhei; got:\n{map}");
    let mut unique = roots.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 3, "each rhei has its own root; got:\n{map}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A plan named the way it is typed from the directory it lives in —
/// `rhei next plan.rhei.md` — has an execution root like any other. Its raw
/// parent is the empty path, and a blank root under `### Reading the rhei`
/// names nothing a reader can open.
// §FS-rhei-memory.3.4
#[test]
fn a_bare_relative_plan_name_still_has_a_root_on_rhei_next() {
    let dir = unique_temp_dir("memory-bare-next");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Bare\n\n## Tasks\n\n### Task 1: Only\n**State:** pending\n",
    );
    write_fixture_file(&dir, "states.yaml", MEMORY_MACHINE);

    // The plan and the machine are named relative to a cwd inside the fixture,
    // which is what a worker standing in the plan's directory types.
    let mut cmd = rhei_command(dir.join(".home"));
    cmd.current_dir(&dir);
    cmd.args(["--state-machine", "states.yaml", "next", "plan.rhei.md", "--task", "1", "--peek"]);
    let output = cmd.output().expect("rhei next should run");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "next should succeed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let map = stdout.split("### Reading the rhei").nth(1).expect("the map is printed");
    let map = map.split("- Under each execution root:").next().expect("the map ends");
    let this_rhei = map.lines().find(|line| line.starts_with("- This rhei: ")).expect("this rhei");
    let listed = map.lines().find(|line| line.starts_with("  - `plan` ")).expect("the rhei list");
    let roots = [
        this_rhei.split('`').nth(1).expect("the root of this rhei"),
        listed.split('`').nth(3).expect("the root in the list"),
    ];
    for root in roots {
        assert!(!root.is_empty(), "a blank execution root names nothing; got:\n{map}");
        assert!(Path::new(root).is_absolute(), "`{root}` is not absolute; got:\n{map}");
        assert!(Path::new(root).exists(), "`{root}` does not exist; got:\n{map}");
    }

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The same bare relative name under `rhei run`: `RHEI_ROOT` is the anchor
/// every relative path in the prompt is resolved against, and the agent log
/// header records what the agent was handed.
// §FS-rhei-memory.3.4 §FS-rhei-agents.8.1
#[test]
fn a_bare_relative_plan_name_exports_a_root_to_the_agent() {
    let (dir, _plan_path, _machine_path) = setup_supervision(
        "memory-bare-run",
        "# Rhei: Bare\n\n## Tasks\n\n### Task 1: Only\n**State:** pending\n",
        MEMORY_MACHINE,
        "",
    );

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.current_dir(&dir);
    cmd.args(["--state-machine", "states.yaml", "run", "plan.rhei.md"]);
    cmd.args(["--no-callbacks", "--no-tui"]);
    let output = cmd.output().expect("rhei run should run");
    assert!(
        output.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let log = fs::read_to_string(dir.join("runtime/logs/task-plan.1-pending.log"))
        .expect("the agent log was written");
    let root = log
        .lines()
        .find_map(|line| line.strip_prefix("rhei_root: "))
        .expect("the header names the root");
    assert!(!root.trim().is_empty(), "a blank RHEI_ROOT anchors nothing; got:\n{log}");
    // The mock agent resolves `$RHEI_ROOT` itself, so the prompt it saved is
    // the proof that what it was handed names the execution root.
    assert!(
        dir.join("runtime/prompts/plan.1-pending-1.md").exists(),
        "the agent wrote under the root it was given"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A mock agent that saves its prompt under the execution root it was handed
/// and writes the result the terminal state needs — and touches nothing under
/// `runtime/logs/`, which is the tree this scenario is about.
const LOG_MAP_AGENT: &str = r#"#!/bin/sh
set -eu
root="${RHEI_ROOT:?}"
task="${RHEI_TASK_ID:?}"
state="${RHEI_STATE:?}"
visit="${RHEI_VISIT_COUNT:-1}"
mkdir -p "$root/runtime/prompts"
prompt=""
while [ $# -gt 0 ]; do
  if [ "$1" = "--prompt" ]; then shift; prompt="${1:-}"; fi
  shift || true
done
printf '%s' "$prompt" > "$root/runtime/prompts/$task-$state-$visit.md"
mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
printf '## Result\n\nTask %s finished %s.\n' "$task" "$state" > "$RHEI_RESULT_PATH"
"#;

/// One `rhei run` writes one log tree, under the root it was started from — the
/// project in a Panta, not the member — so the map names that directory rather
/// than promising `runtime/logs/` under every execution root.
// §FS-rhei-memory.2 §FS-rhei-memory.3.4
#[test]
fn the_map_names_the_log_directory_the_run_writes() {
    let dir = unique_temp_dir("memory-log-map");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Two Roots\n");
    let machine_path = write_fixture_file(&dir, "states.yaml", MEMORY_MACHINE);
    let script = write_fixture_file(&dir, "mock-agent.sh", LOG_MAP_AGENT);
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("settings dir");
    let script_json = serde_json::to_string(&script.display().to_string()).expect("script json");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{ "mock": {{ "command": ["sh", {script_json}], "prompt_flag": "--prompt", "timeout": "30s" }} }}
}}"#
        ),
    )
    .expect("write settings");
    fs::create_dir_all(dir.join("alpha/tasks")).expect("member dirs");
    fs::write(dir.join("alpha/index.rhei.md"), "# Rhei: Alpha\n").expect("write index");
    fs::write(dir.join("alpha/tasks/t.md"), "### Task 1: Work alpha\n**State:** pending\n")
        .expect("write task file");

    let mut cmd = rhei_command(dir.join(".home"));
    cmd.arg("--state-machine").arg(&machine_path).arg("run").arg(&dir);
    cmd.args(["--no-callbacks", "--no-tui"]);
    let output = cmd.output().expect("rhei run should run");
    assert!(
        output.status.success(),
        "run should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // The results sit under the member's execution root …
    assert!(dir.join("alpha/runtime/results/alpha.1.md").exists(), "the member owns its results");
    // … and the transcripts do not: they are the run's, at the root it started
    // from. §FS-rhei-agents.8
    let logs = dir.join("runtime/logs");
    assert!(logs.join("task-alpha.1-pending.log").exists(), "the run writes its logs here");
    assert!(!dir.join("alpha/runtime/logs").exists(), "and nothing under the member");

    let prompt = fs::read_to_string(dir.join("alpha/runtime/prompts/alpha.1-review-1.md"))
        .expect("the review prompt was saved");
    assert!(
        prompt.contains(&format!("- Agent transcripts: `{}`\n", logs.display())),
        "the map names the directory that exists; got:\n{prompt}"
    );
    assert!(
        !prompt.contains("`runtime/logs/` (agent transcripts)"),
        "the map no longer claims a per-root log tree; got:\n{prompt}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// A counted `review` loop, so a ticket's `**State:**` carries a `-<visit>`
/// suffix that the machine itself never names.
const COUNTED_MACHINE: &str = r#"name: counted-e2e
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    instructions: Do the work for Task {task_id}.
  review:
    description: Review
    visits: 3
    instructions: Review Task {task_id}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: review, description: Work done }
  - { from: review, to: review, description: Another round }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// One `rhei next` screen spells a state one way: the machine's own name, which
/// is the only form `rhei transition --from` accepts. The visit belongs to
/// `## Position`, and `--json` still reports the authored value.
// §FS-rhei-next.4.1 §FS-rhei-memory.3.1
#[test]
fn rhei_next_prints_one_spelling_of_the_state() {
    let dir = unique_temp_dir("memory-one-spelling");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Looper\n\n## Tasks\n\n### Task 1: Looper\n**State:** review-3\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", COUNTED_MACHINE);

    let peek = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek"]);
    assert_success(&peek);
    assert!(
        peek.stdout.starts_with(
            "Task plan.1 \u{2014} current state: 'review' (read-only peek; not advanced)\n"
        ),
        "got:\n{}",
        peek.stdout
    );
    assert!(peek.stdout.contains("--- Instructions (review) ---\n"), "got:\n{}", peek.stdout);
    assert!(
        peek.stdout.contains("**Task plan.1: Looper [review]** \u{2190} this invocation (visit 3)"),
        "got:\n{}",
        peek.stdout
    );
    assert!(!peek.stdout.contains("review-3"), "one spelling only; got:\n{}", peek.stdout);

    // The authored form is data, not display: `--json` keeps it.
    let json = run_cli("next", &plan_path, &machine_path, &["--task", "1", "--peek", "--json"]);
    assert_success(&json);
    let payload: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("next --json parses");
    assert_eq!(payload["state"], "review-3", "got: {payload}");
    assert_eq!(payload["from_state"], "review-3", "got: {payload}");

    fs::remove_dir_all(dir).expect("cleanup");
}
