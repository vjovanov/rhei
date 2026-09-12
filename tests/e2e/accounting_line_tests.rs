//! Black-box proof that a line-oriented frontend writes one accounting line per
//! invocation, carrying what that invocation finally cost.
//! §FS-rhei-cost-accounting.7.1 §FS-rhei-run-json.2.1
//!
//! The agent here prints **two** `turn.completed` lines on stdout, and that is
//! the whole point of the fixture. The other accounting fixtures have their
//! mock agent write the capture file directly, which never reaches the
//! streaming emitter, so no test in this suite could see a duplicated line. A
//! two-turn agent makes the streamed reports and the final report carry
//! *different* figures, which is what tells a sink that prints the last report
//! from one that prints the first.

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
use super::*;

#[cfg(unix)]
const PRICED_MACHINE: &str = r#"name: line-oriented-accounting
version: 1
models: [luna]
states:
  work:
    initial: true
    description: Measure two turns and finish
    agent: codex
    model: luna
    agent_timeout: 10s
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

#[cfg(unix)]
const ONE_TASK_PLAN: &str = r#"# Rhei: Line-Oriented Accounting

## Tasks

### Task 1: Measure two turns
**State:** work
"#;

/// What the first turn alone prices to, and so what a streamed report carries
/// while the agent is still working. A sink that keeps the *first* report
/// prints this: one line, a plausible-looking figure, and lower than the
/// invocation cost. §FS-rhei-cost-accounting.7.1
#[cfg(unix)]
const FIRST_TURN_COST: &str = "$9.62";

/// What both turns together price to: the figure the durable record carries and
/// the only one that may reach the log. §FS-rhei-cost-accounting.7.1
#[cfg(unix)]
const INVOCATION_COST_MICRO: u64 = 11_550_000;
#[cfg(unix)]
const INVOCATION_COST: &str = "$11.55";

/// A price book in USD, so the printed line reads `$` the way the ticket's own
/// run did. The rates are the fixture rates the pricing scenarios already use.
#[cfg(unix)]
fn write_price_book(dir: &Path) -> PathBuf {
    let path = dir.join("luna-prices.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "rhei.accounting.prices.v1",
            "price_book_id": "fixture-luna-two-turn",
            "currency": "USD",
            "entries": [{
                "provider": "openai",
                "model": "gpt-5.6-luna",
                "effective_at": "2026-09-01T00:00:00Z",
                "unit": "1m_tokens",
                "input_total_micro": 2_000_000,
                "input_cached_read_micro": 250_000,
                "input_cache_write_micro": 4_000_000,
                "output_total_micro": 10_000_000
            }]
        }))
        .expect("serialize price book"),
    )
    .expect("write price book");
    path
}

/// A `codex`-shaped agent that reports two turns on **stdout**, so the run's
/// streaming extractor sees each of them and the invocation's cost is only
/// settled once the second has arrived.
///
/// Turn 1 prices to `$9.62`; turn 2 adds `$1.93`; the invocation is `$11.55`.
/// §FS-rhei-cost-accounting.4
#[cfg(unix)]
fn write_two_turn_codex_settings(root: &Path) {
    let script = write_python_agent(
        root,
        "two-turn-codex.py",
        r#"import json
import time

print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': 1250000,
        'cached_input_tokens': 500000,
        'cache_creation_input_tokens': 250000,
        'output_tokens': 750000,
    },
}), flush=True)
time.sleep(0.1)
print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': 250000,
        'cached_input_tokens': 100000,
        'cache_creation_input_tokens': 50000,
        'output_tokens': 150000,
    },
}), flush=True)
time.sleep(0.1)
result('## Result\n\nTwo measured turns completed.\n')
"#,
    );
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "codex", "model": "luna", "agent_timeout": "10s" }},
  "agents": {{
    "codex": {{ "command": {}, "prompt_flag": "--prompt", "timeout": "10s" }}
  }},
  "models": {{
    "luna": {{ "provider": "openai", "model": "gpt-5.6-luna", "default_agent": "codex" }}
  }}
}}"#,
            fixture_command(&script)
        ),
    )
    .expect("write agent settings");
}

/// One workspace holding the plan, the machine, the price book, and the agent.
#[cfg(unix)]
fn two_turn_workspace(prefix: &str) -> (TestDir, PathBuf, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TASK_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", PRICED_MACHINE);
    let prices = write_price_book(&dir);
    write_two_turn_codex_settings(&dir);
    (dir, plan, machine, prices)
}

/// Every `accounting:` line the frontend wrote, in the order it wrote them.
#[cfg(unix)]
fn accounting_lines(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|line| line.starts_with("accounting:")).collect()
}

/// The one invocation record the run wrote.
#[cfg(unix)]
fn sole_invocation(root: &Path) -> serde_json::Value {
    let directory = root.join("runtime/accounting/invocations");
    let mut records: Vec<PathBuf> = fs::read_dir(&directory)
        .unwrap_or_else(|err| panic!("read {}: {err}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    records.sort();
    assert_eq!(records.len(), 1, "the run spawned one agent, so it wrote one record");
    serde_json::from_str(&fs::read_to_string(&records[0]).expect("read invocation"))
        .expect("parse invocation")
}

/// Every `usage_reported` record the run's durable event log holds.
#[cfg(unix)]
fn usage_reported_records(root: &Path) -> Vec<serde_json::Value> {
    let path = root.join("runtime/events.jsonl");
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["event"] == "usage_reported")
        .collect()
}

/// One invocation, one `accounting:` line, and the figure on it is what the
/// invocation cost rather than what it had cost so far.
///
/// The three assertions are ordered so the discriminating one is read first.
/// A sink that kept the *first* usage report would satisfy the line count and
/// still print `$9.62`, which is a running total observed halfway through the
/// invocation and lower than the invocation's own cost — the first assertion is
/// the one that rejects it.
// §FS-rhei-cost-accounting.7.1 §FS-rhei-cost-accounting.1
#[cfg(unix)]
#[test]
fn one_accounting_line_carries_the_invocations_final_cost() {
    let (dir, plan, machine, prices) = two_turn_workspace("accounting-line-final");
    let prices_arg = prices.to_string_lossy().into_owned();

    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &prices_arg]);

    assert_success(&result);
    let lines = accounting_lines(&result.stdout);

    assert!(
        !lines.iter().any(|line| line.contains(FIRST_TURN_COST)),
        "no accounting line may carry the running total {FIRST_TURN_COST}; \
         only the invocation's final cost {INVOCATION_COST} may be printed, and got {lines:#?}"
    );
    assert_eq!(lines.len(), 1, "one invocation writes one accounting line, and got {lines:#?}");
    assert!(
        lines[0].contains(INVOCATION_COST),
        "the line carries the invocation's final cost {INVOCATION_COST}, and got {:?}",
        lines[0]
    );

    let invocation = sole_invocation(&dir);
    assert_eq!(
        invocation["pricing"]["amount_micro"], INVOCATION_COST_MICRO,
        "the durable record prices both turns together"
    );
    assert_eq!(
        invocation["pricing"]["currency"], "USD",
        "the printed figure and the record are in the same currency"
    );
}

/// Every usage report says which it is, and exactly one of them is `final`.
///
/// The streamed reports are what keeps the keyed frontends showing spend while
/// work is still running, so this pins that they are still emitted as well as
/// that the line-oriented sink has one report to choose.
// §FS-rhei-run-json.2.1 §FS-rhei-cost-accounting.7.1
#[cfg(unix)]
#[test]
fn usage_reports_name_themselves_and_exactly_one_is_final() {
    let (dir, plan, machine, prices) = two_turn_workspace("accounting-line-reports");
    let prices_arg = prices.to_string_lossy().into_owned();

    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &prices_arg]);

    assert_success(&result);
    let records = usage_reported_records(&dir);
    let reports: Vec<&str> =
        records.iter().map(|record| record["report"].as_str().unwrap_or("<missing>")).collect();

    assert_eq!(
        reports.iter().filter(|report| **report == "final").count(),
        1,
        "exactly one report per invocation is final, and got {reports:?}"
    );
    assert_eq!(
        reports.iter().filter(|report| **report == "streamed").count(),
        2,
        "each measured turn still reports while the agent runs, and got {reports:?}"
    );
    assert_eq!(
        reports.last().copied(),
        Some("final"),
        "the final report is the last one the invocation emits, and got {reports:?}"
    );

    let final_record = records
        .iter()
        .find(|record| record["report"] == "final")
        .expect("the invocation emitted a final report");
    assert_eq!(
        final_record["usage"]["cost_micro"], INVOCATION_COST_MICRO,
        "the final report carries the invocation's own cost"
    );
}
