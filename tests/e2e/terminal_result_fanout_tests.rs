//! One result per ticket, when the state fans out over several invocations.
//!
//! Every invocation writes its own fragment and `run` merges them before the
//! terminal transition; a shared path would have let the last writer erase
//! every sibling's account.

// §FS-rhei-states.3.3 §FS-rhei-agents.3.2

use std::fs;
use std::path::Path;

use super::*;

/// A fanned-out state gives every invocation its own result fragment, and `run`
/// merges them into the ticket's result before the terminal transition. One
/// shared path would have made the last writer erase every sibling's account.
// §FS-rhei-states.3.3 §FS-rhei-run.3
const FANOUT_TERMINAL_MACHINE: &str = r#"name: fanout-terminal-result
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: Every reviewer weighs in
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: review
    to: completed
"#;

pub(super) const FANOUT_PLAN: &str = r#"# Rhei: Fanout Result

## Tasks

### Task 1: Review from every angle
**State:** review
"#;

/// Settings whose `mock` agent runs `script`, with a model registry both fan-out
/// targets resolve through.
pub(super) fn write_fanout_agent_settings(workspace_root: &Path, script: &Path) {
    let settings_dir = workspace_root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "10s" }},
  "agents": {{
    "mock": {{ "command": {command}, "stdin_prompt": true, "timeout": "10s" }}
  }},
  "models": {{
    "alpha": {{ "provider": "mock", "model": "alpha", "default_agent": "mock" }},
    "beta": {{ "provider": "mock", "model": "beta", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
}

#[test]
fn a_fanned_out_terminal_edge_keeps_every_invocation_s_account() {
    let dir = unique_temp_dir("terminal-result-fanout");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TERMINAL_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"result('{} reviewed it.\n'.format(env('RHEI_MODEL')))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    // Each invocation wrote its own fragment, keyed by its identity …
    for identity in ["mock-mock-alpha", "mock-mock-beta"] {
        assert!(
            dir.join(format!("runtime/results/plan.1/review/1/{identity}.md")).exists(),
            "{identity}: fan-out invocation writes its own fragment, keyed by state and visit"
        );
    }

    // … and the merged result carries both, attributed, in declared order.
    let merged =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("merged result file");
    assert!(merged.contains("alpha reviewed it."), "model-a's account survives; got:\n{merged}");
    assert!(merged.contains("beta reviewed it."), "model-b's account survives; got:\n{merged}");
    assert!(
        merged.contains("## Result \u{2014} mock-mock-alpha")
            && merged.contains("## Result \u{2014} mock-mock-beta"),
        "each entry names the invocation it came from; got:\n{merged}"
    );
    assert!(
        merged.find("mock-mock-alpha") < merged.find("mock-mock-beta"),
        "entries follow declared invocation order; got:\n{merged}"
    );
}

/// The completion condition is per invocation, so a fan-out worker that writes
/// nothing fails its own — the sibling that did write is not an answer for it.
// §FS-rhei-agents.3.2 §FS-rhei-states.3.3
#[test]
fn a_fanned_out_invocation_that_writes_nothing_fails_its_own_completion_condition() {
    let dir = unique_temp_dir("terminal-result-fanout-silent");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TERMINAL_MACHINE);
    // Only `alpha` answers.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"if env('RHEI_MODEL') == 'alpha':
    result('alpha reviewed it.\n')
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(
        !result.status.success(),
        "the silent invocation must fail its own condition\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let combined = format!("{}{}", result.stdout, result.stderr);
    let owed = Path::new("plan.1").join("review").join("1").join("mock-mock-beta.md");
    assert!(
        combined.contains(&owed.display().to_string()),
        "the warning names the fragment the silent invocation owed; got:\n{combined}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "review");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "nothing is merged when the state did not finish"
    );
}

/// The same fan-out state, but the terminal state demands an `inputs:` artifact
/// nothing writes, so the move is refused after the fragments are merged. The
/// merge must survive that and must not re-append itself on the next attempt.
// §FS-rhei-states.3.3
const FANOUT_REFUSED_MACHINE: &str = r#"name: fanout-refused
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: Every reviewer weighs in
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  completed:
    final: true
    description: Done
    inputs:
      - name: sign-off
        path: runtime/sign-off/{task_id}.md
        required: true
transitions:
  - from: review
    to: completed
"#;

/// A fan-out result is merged **once**, when the last fragment lands — not once
/// per invocation that exits, and not again on a retry over the same fragments.
/// Appending per invocation left four entries for a ticket that never moved.
// §FS-rhei-states.3.3
#[test]
fn a_refused_fan_out_move_merges_the_fragments_exactly_once_per_attempt() {
    let dir = unique_temp_dir("terminal-result-fanout-once");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_REFUSED_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"result('{} reviewed it.\n'.format(env('RHEI_MODEL')))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let first = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!first.status.success(), "the missing target input refuses the move");
    assert_task_state(&plan_path, &machine_path, "1", "review");

    let entries = |label: &str| {
        let merged = fs::read_to_string(dir.join("runtime/results/plan.1.md"))
            .unwrap_or_else(|err| panic!("{label}: merged result file: {err}"));
        assert!(
            merged.matches("## Result \u{2014} mock-mock-alpha").count() == 1
                && merged.matches("## Result \u{2014} mock-mock-beta").count() == 1,
            "{label}: one entry per invocation, no more; got:\n{merged}"
        );
    };
    entries("first pass");

    // A second run rewrites the same fragments, so the merged block is the one
    // already on disk and nothing is appended.
    let second = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(!second.status.success(), "still refused");
    entries("second pass");
}

/// Two fanned-out states over the same targets. Keyed by identity alone, every
/// `refine` invocation would find `review`'s fragment already on disk, write
/// nothing, and hand the ticket `review`'s account as its result.
// §FS-rhei-states.3.3 §FS-rhei-agents.3.2
const FANOUT_TWO_STATE_MACHINE: &str = r#"name: fanout-two-states
version: 1
models:
  - alpha
  - beta
states:
  review:
    initial: true
    description: First look
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  refine:
    description: Second look
    all_targets:
      - "mock:mock:alpha"
      - "mock:mock:beta"
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: review
    to: refine
  - from: refine
    to: completed
"#;

#[test]
fn a_second_fanned_out_state_does_not_inherit_the_first_s_fragments() {
    let dir = unique_temp_dir("terminal-result-fanout-stale");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TWO_STATE_MACHINE);
    // Seed fragments from an earlier state. A non-terminal state is not asked
    // to write them, but stale files can still survive an interrupted or older
    // run and must never satisfy a later state's contract.
    let stale_dir = dir.join("runtime/results/plan.1/review/1");
    fs::create_dir_all(&stale_dir).expect("create stale fragment directory");
    for identity in ["mock-mock-alpha", "mock-mock-beta"] {
        fs::write(stale_dir.join(format!("{identity}.md")), format!("STALE by {identity}.\n"))
            .expect("seed stale result fragment");
    }
    // `refine` can finish the task, but exits 0 having written nothing.
    let agent = write_python_agent(&dir, "mock-agent.py", "");
    write_fanout_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert!(
        !result.status.success(),
        "`refine` wrote nothing, so it fails its own completion condition\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "refine");
    let combined = format!("{}{}", result.stdout, result.stderr);
    let owed = Path::new("plan.1").join("refine").join("1").join("mock-mock-alpha.md");
    assert!(
        combined.contains(&owed.display().to_string()),
        "the warning names the fragment `refine` owed, under its own state; got:\n{combined}"
    );
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "the ticket never finished, so it has no result — least of all `review`'s"
    );
    for identity in ["mock-mock-alpha", "mock-mock-beta"] {
        assert!(
            dir.join(format!("runtime/results/plan.1/review/1/{identity}.md")).exists(),
            "{identity}: `review`'s fragment stays where `review` put it"
        );
    }
}

/// One invocation finishing is not the state finishing. With a sibling still
/// running, attempting the merge produced `1 of its fan-out invocation(s) wrote
/// no result` on a run where nothing was wrong.
// §FS-rhei-states.3.3
#[test]
fn a_slow_fan_out_sibling_does_not_raise_a_false_alarm() {
    let dir = unique_temp_dir("terminal-result-fanout-slow");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", FANOUT_PLAN);
    let machine_path = write_fixture_file(&dir, "states.yaml", FANOUT_TERMINAL_MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        r#"model = env('RHEI_MODEL')
if model == 'beta':
    time.sleep(2)
result('{} reviewed it.\n'.format(model))
"#,
    );
    write_fanout_agent_settings(&dir, &agent);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_task_state(&plan_path, &machine_path, "1", "completed");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("wrote no result"),
        "a healthy run must not accuse the sibling that had not finished yet; got:\n{combined}"
    );
    let merged =
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("merged result file");
    assert!(
        merged.contains("alpha reviewed it.") && merged.contains("beta reviewed it."),
        "both accounts survive; got:\n{merged}"
    );
}
