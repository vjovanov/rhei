//! The workspaces the cost-accounting scenarios drive, and the numbers they
//! assert on.
//!
//! `fixtures/accounting-archive/` holds six invocation records this machine
//! really wrote, copied out of `~/f/.accounting-archive/`: three by `codex` and
//! three by `claude-code`, none of them naming a run because no record did
//! before this change. They are the history the change has to keep reading, so
//! the tests read copies of the real thing rather than records invented to suit
//! them. §FS-rhei-cost-accounting.3.5

use std::fs;
use std::path::{Path, PathBuf};

use super::{fixture_command, fixture_path, unique_temp_dir, write_python_agent, TestDir};

/// How many archived records the fixture set holds.
pub const ARCHIVED_RECORDS: u64 = 6;

/// `tokens.total` summed over the fixture set. Five of the six are `measured`;
/// the sixth is `no-usage-emitted` and contributes a missing count instead of a
/// number, which is what makes the set's coverage `partial`.
pub const ARCHIVED_TOTAL_TOKENS: u64 = 1_714_295;

/// The fixture set's `tokens.total` per UTC calendar day. The three days are
/// what `--by day` and a `--since`/`--until` window are asserted against.
pub const ARCHIVED_TOKENS_2026_08_31: u64 = 1_446_820;
pub const ARCHIVED_TOKENS_2026_09_01: u64 = 129_035;
pub const ARCHIVED_TOKENS_2026_09_02: u64 = 138_440;

/// What the mock agent reports as its own usage, so a run's own total is a
/// number no archived record could produce.
pub const MOCK_AGENT_TOTAL_TOKENS: u64 = 4321;

/// A plan whose only ticket is already terminal: the run enters agent mode,
/// spawns nothing, and its report is the one the ticket is about.
pub const TERMINAL_PLAN: &str = r#"# Rhei: Accounting Probe

## Tasks

### Task 1: Nothing left to do
**State:** completed
"#;

/// The same plan with work left, so exactly one agent runs and exactly one
/// invocation record is written for it.
pub const WORKING_PLAN: &str = r#"# Rhei: Accounting Probe

## Tasks

### Task 1: Do the work
**State:** work
"#;

/// A machine whose working state names an agent, so `rhei run` enters agent
/// mode whether or not any ticket is left to advance.
///
/// Two names in it are load-bearing. The agent is spelled `claude-code`
/// because the accounting extractor is chosen by the agent's id, so a profile
/// under that name is how a test gets a real invocation record without a real
/// Claude Code on the machine. §FS-rhei-cost-accounting.4 The model is spelled
/// `anthropic:claude-sonnet-4-6` because that is the built-in price book's one
/// entry, so the run's own record prices and its coverage reads `complete` —
/// which is the reading §FS-rhei-cost-accounting.6.2 demotes. No test asserts
/// on the amount, only on the coverage word.
pub const ACCOUNTING_MACHINE: &str = r#"name: accounting-probe
version: 1
models: [claude-sonnet-4-6]
states:
  work:
    initial: true
    description: Work to do
    target: claude-code[yolo]:anthropic:claude-sonnet-4-6
    agent_timeout: 60s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

/// A stand-in for Claude Code that reports usage through the capture contract
/// and nothing else. §FS-rhei-cost-accounting.4
const USAGE_REPORTING_AGENT: &str = r#"capture = env('RHEI_ACCOUNTING_USAGE_PATH')
if capture:
    append(
        capture,
        '{"schema": "rhei.accounting.usage.v1", "usage": '
        '{"total_tokens": 4321, "input_tokens": 4000, '
        '"cached_input_tokens": 3000, "output_tokens": 321}}\n',
    )
result('## Result\n\nMock agent finished.\n')
"#;

/// One workspace: a plan, the machine above, and a settings file whose
/// `claude-code` profile is the fixture agent. Returns the temp directory, the
/// plan path, and the machine path, in the shape the rest of the harness takes.
pub fn accounting_workspace(prefix: &str, plan: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let agent_script = write_python_agent(&dir, "mock-claude-code.py", USAGE_REPORTING_AGENT);
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let command = fixture_command(&agent_script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "claude-code": {{
      "command": {command},
      "timeout": "60s",
      "stdin_prompt": true,
      "intervene_stdin": false,
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "claude-sonnet-4-6": {{
      "provider": "anthropic",
      "model": "claude-sonnet-4-6",
      "default_agent": "claude-code"
    }}
  }}
}}"#
        ),
    )
    .expect("write settings");
    let plan_path = dir.join("plan.rhei.md");
    fs::write(&plan_path, plan).expect("write plan");
    let machine_path = dir.join("states.yaml");
    fs::write(&machine_path, ACCOUNTING_MACHINE).expect("write machine");
    (dir, plan_path, machine_path)
}

/// Copy the archived records into a workspace's accounting directory, the way a
/// workspace that has been worked in for a while already holds them.
pub fn seed_archived_records(workspace_root: &Path) {
    let target = workspace_root.join("runtime/accounting/invocations");
    fs::create_dir_all(&target).expect("create invocations dir");
    let mut seeded = 0;
    for entry in fs::read_dir(fixture_path("accounting-archive")).expect("read archive fixtures") {
        let entry = entry.expect("archive fixture entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        fs::copy(&path, target.join(entry.file_name())).expect("copy archived record");
        seeded += 1;
    }
    assert_eq!(seeded, ARCHIVED_RECORDS, "the fixture set is six archived records");
}

/// Every invocation record the workspace holds, parsed.
pub fn invocation_records(workspace_root: &Path) -> Vec<serde_json::Value> {
    let dir = workspace_root.join("runtime/accounting/invocations");
    let mut records = Vec::new();
    for entry in fs::read_dir(&dir).expect("read invocation records") {
        let path = entry.expect("invocation entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read invocation record");
        records.push(serde_json::from_str(&text).expect("invocation record parses"));
    }
    records
}

/// The id of the run the workspace last held, read from its own descriptor.
/// §FS-rhei-run-headless.2
pub fn last_run_id(workspace_root: &Path) -> String {
    let text = fs::read_to_string(workspace_root.join("runtime/run.json"))
        .expect("run descriptor written");
    let descriptor: serde_json::Value = serde_json::from_str(&text).expect("run descriptor parses");
    descriptor["id"].as_str().expect("run descriptor names the run").to_string()
}

/// The durable Markdown report of the run that just finished.
pub fn run_report(workspace_root: &Path) -> String {
    fs::read_to_string(workspace_root.join("runtime/run-report.md")).expect("run report written")
}
