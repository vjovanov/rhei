//! What happens to a prompt too large for a command line. agent-grounds/rhei#179:
//! a supervisor brief past the platform's limit aborted the spawn with
//! `Argument list too long`, and the help sent the user to `PATH` for a binary
//! that was plainly there.
//!
//! Both tests drive a registered agent rather than the built-in `claude-code`,
//! because reaching that one needs a stub `claude` on `PATH` and
//! §REQ-cross-platform.4 keeps a shell script out of a fixture. That the
//! built-in uses the stdin transport is pinned in the CLI's own unit tests; what
//! is pinned here is that the transport survives the size, and that the agents
//! still bounded by it say so.

use std::fs;

use super::*;

/// Larger than every supported platform's limit: per §FS-rhei-errors.7.1 Linux
/// rejects a single argument of 131072 bytes or more, Windows a command line above
/// 32767 characters, and macOS a total above roughly one megabyte. One size
/// clears all three, so neither test is vacuous on any of them.
const OVERSIZED_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Present in every line of the task body, so "the prompt reached `argv`" is a
/// substring search rather than a length comparison.
const MARKER: &str = "OVERSIZED-PROMPT-MARKER";

const MACHINE: &str = r#"name: oversized-prompt
version: 1
states:
  work:
    initial: true
    description: Carry the brief
    instructions: |
      Read the brief and finish.
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

fn oversized_plan() -> String {
    let line = format!("{MARKER} filler that makes this task body larger than a command line.\n");
    let body = line.repeat(OVERSIZED_BODY_BYTES / line.len() + 1);
    let heading = "# Rhei: Oversized Prompt\n\n## Tasks\n\n";
    format!("{heading}### Task 1: Carry a very large brief\n**State:** work\n\n{body}")
}

/// A fixture that reports how the prompt reached it. The prelude's
/// `agent_prompt()` already resolves either transport, so what this adds is the
/// argument vector it was spawned with.
fn write_recording_agent(dir: &std::path::Path, record: &std::path::Path) -> std::path::PathBuf {
    let preamble = format!(
        "import json\nRECORD = {}\nMARKER = {}\n",
        serde_json::to_string(&record.display().to_string()).expect("record path json"),
        serde_json::to_string(MARKER).expect("marker json"),
    );
    write_python_agent(
        dir,
        "recording-agent.py",
        &format!(
            "{preamble}{}",
            r#"
prompt = agent_prompt()
write(pathlib.Path(RECORD), json.dumps({
    'prompt_bytes': len(prompt.encode('utf-8')),
    'prompt_in_argv': any(MARKER in arg for arg in sys.argv[1:]),
    'argv': sys.argv[1:],
}))
result('## Result\n\nThe brief arrived whole.\n')
"#
        ),
    )
}

fn write_settings(root: &std::path::Path, agent_id: &str, entry: &str) {
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "{agent_id}", "agent_timeout": "60s" }},
  "agents": {{ "{agent_id}": {entry} }}
}}"#
        ),
    )
    .expect("write settings");
}

/// The guard, not the proof: this transport already works today, and the point
/// of the ticket's fix is to put `claude-code` on it. What it holds is that
/// prompt size stops being a spawn concern once the prompt travels on stdin —
/// no cap applies, and the brief arrives whole rather than truncated.
// §FS-rhei-agents.2.2 §FS-rhei-agents.1.1.2
#[test]
fn a_brief_past_every_platform_limit_reaches_a_stdin_agent_whole() {
    let dir = unique_temp_dir("oversized-prompt-stdin");
    let record = dir.join("delivery.json");
    let agent = write_recording_agent(&dir, &record);
    write_settings(
        &dir,
        "stdin-agent",
        &format!(
            r#"{{ "command": {}, "stdin_prompt": true, "timeout": "60s" }}"#,
            fixture_command(&agent)
        ),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &oversized_plan());
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&run);

    let delivered: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&record).expect("delivery record"))
            .expect("delivery record json");
    assert_eq!(
        delivered["prompt_in_argv"],
        serde_json::Value::Bool(false),
        "a stdin transport puts no part of the prompt on the command line: {delivered}"
    );
    let delivered_bytes = delivered["prompt_bytes"].as_u64().expect("prompt_bytes") as usize;
    assert!(
        delivered_bytes >= OVERSIZED_BODY_BYTES,
        "the whole brief must arrive: {delivered_bytes} bytes of at least {OVERSIZED_BODY_BYTES}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");
}

/// The failure the ticket is about, seen from the outside. An agent that
/// carries its prompt in `argv` still cannot take a brief this size — that is
/// the platform's rule, not rhei's — but the failure now says the size is why,
/// instead of sending the reader to `PATH` for a binary that ran a moment ago.
// §FS-rhei-errors.7 §FS-rhei-errors.1.2
#[test]
fn an_argv_agent_past_the_limit_blames_the_prompt_size_not_the_path() {
    let dir = unique_temp_dir("oversized-prompt-argv");
    let record = dir.join("delivery.json");
    let agent = write_recording_agent(&dir, &record);
    write_settings(
        &dir,
        "argv-agent",
        &format!(
            r#"{{ "command": {}, "prompt_flag": "--prompt", "timeout": "60s" }}"#,
            fixture_command(&agent)
        ),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &oversized_plan());
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert!(!run.status.success(), "the spawn cannot succeed:\n{}", run.stdout);

    // Miette wraps both the message and the help, so every claim below is made
    // against the diagnostic with its line breaks collapsed.
    let said = run.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !said.contains("Check it exists on PATH"),
        "the binary is present; the PATH remedy is the wrong next action:\n{}",
        run.stderr
    );
    assert!(
        said.contains("stdin"),
        "the remedy is prompt delivery, so it names stdin:\n{}",
        run.stderr
    );

    let sizes: Vec<usize> = said
        .split(" bytes")
        .filter_map(|before| before.rsplit(' ').next())
        .filter_map(|word| word.parse().ok())
        .collect();
    assert!(
        sizes.iter().any(|size| *size >= OVERSIZED_BODY_BYTES),
        "the failure states the composed prompt's size; found {sizes:?} in:\n{}",
        run.stderr
    );
}
