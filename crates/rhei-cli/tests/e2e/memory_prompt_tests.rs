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
