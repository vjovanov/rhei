// The map at the end of every prompt, driven end to end (FS-rhei-memory.3.4) —
// every path absolute on `rhei next`, a root for a bare relative plan name on
// both surfaces, and the transcripts directory the run actually writes.
//
// Its own part beside `memory_prompt_tests.rs`: those tests read the sections,
// these read the paths the sections name.

// §AR-source-file-size.3 §FS-rhei-memory.3.4

use std::fs;

use super::memory_prompt_tests::MEMORY_MACHINE;
use super::supervision_tests::setup_supervision;
use super::*;

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
    let paths: Vec<&str> =
        quoted.iter().copied().filter(|token| token.contains(std::path::MAIN_SEPARATOR)).collect();
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
}

/// A mock agent that saves its prompt under the execution root it was handed
/// and writes the result the terminal state needs — and touches nothing under
/// `runtime/logs/`, which is the tree this scenario is about.
const LOG_MAP_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')

prompt = ''
args = sys.argv[1:]
while args:
    if args.pop(0) == '--prompt' and args:
        prompt = args.pop(0)
write(root / 'runtime' / 'prompts' / '{}-{}-{}.md'.format(task, state, visit), prompt)

result('## Result\n\nTask {} finished {}.\n'.format(task, state))
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
    let script = write_python_agent(&dir, "mock-agent.py", LOG_MAP_AGENT);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("settings dir");
    let command = fixture_command(&script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{ "mock": {{ "command": {command}, "prompt_flag": "--prompt", "timeout": "30s" }} }}
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
    let transcripts = prompt
        .lines()
        .find_map(|line| line.strip_prefix("- Agent transcripts: `"))
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or_else(|| panic!("the map names a transcripts directory; got:\n{prompt}"));
    assert_eq!(
        std::path::Path::new(transcripts).canonicalize().expect("the named directory exists"),
        logs.canonicalize().expect("the log directory exists"),
        "the map names the directory that exists; got:\n{prompt}"
    );
    // … spelled the way the same map spells every other root. A temp dir has
    // two spellings on macOS (`/var/…`, `/private/var/…`); one prompt uses one.
    // §FS-rhei-memory.1.2
    let this_rhei = prompt
        .lines()
        .find_map(|line| line.strip_prefix("- This rhei: `"))
        .and_then(|rest| rest.split('`').next())
        .expect("the map names this rhei's root");
    let project_root =
        std::path::Path::new(this_rhei).parent().expect("the member sits under the project");
    assert!(
        std::path::Path::new(transcripts).starts_with(project_root),
        "one spelling per prompt: transcripts `{transcripts}` beside root `{this_rhei}`"
    );
    assert!(
        !prompt.contains("`runtime/logs/` (agent transcripts)"),
        "the map no longer claims a per-root log tree; got:\n{prompt}"
    );
}
