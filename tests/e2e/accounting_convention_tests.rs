// §AR-source-file-size.3

//! What `input.total` means, and what it costs. One run spawns a
//! `codex`-shaped agent and a `claude-code`-shaped one; both records have to
//! say the same thing about cached input, and neither may be charged for it
//! twice. Then the other half: an archive of records written before any of this
//! was stated is read under the convention each record's own agent implies,
//! rather than at face value.
//! §FS-rhei-cost-accounting.3.1 §FS-rhei-cost-accounting.3.6

use std::fs;
use std::path::{Path, PathBuf};

use super::accounting_support::{accounting_workspace, invocation_records, TERMINAL_PLAN};
use super::{
    assert_success, fixture_command, fixture_path, run_cli, unique_temp_dir, write_fixture_file,
    write_python_agent, TestDir,
};

/// One task per agent, so one run writes one record of each shape.
const MIXED_PLAN: &str = r#"# Rhei: Two Agents, One Convention

## Tasks

### Task 1: Codex-shaped usage
**State:** codex

### Task 2: Claude-shaped usage
**State:** claude
"#;

const MIXED_MACHINE: &str = r#"name: mixed-convention
version: 1
models: [sol, sonnet]
states:
  codex:
    initial: true
    description: Codex-shaped usage
    agent: codex
    model: sol
    agent_timeout: 20s
  claude:
    description: Claude-shaped usage
    agent: claude-code
    model: sonnet
    agent_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: codex
    to: completed
  - from: claude
    to: completed
"#;

/// The reproduction's own two invocations, in each provider's dialect.
///
/// `codex` reports a whole-prompt `input_tokens` of 1,000 with 700 of it
/// cached; `claude-code` reports 100 fresh input tokens beside 700 cached
/// reads. Those are different turns, and that is the point: whatever the
/// provider's arithmetic, the record that comes out states one convention, and
/// each cached read is charged exactly once.
const CODEX_AGENT: &str = r#"import json

print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': 1000,
        'cached_input_tokens': 700,
        'output_tokens': 50,
    },
}), flush=True)
result('## Result\n\nCodex-shaped invocation finished.\n')
"#;

const CLAUDE_AGENT: &str = r#"import json

print(json.dumps({
    'type': 'result',
    'subtype': 'success',
    'is_error': False,
    'result': 'Claude-shaped invocation finished.',
    'usage': {
        'input_tokens': 100,
        'cache_read_input_tokens': 700,
        'cache_creation_input_tokens': 0,
        'output_tokens': 50,
    },
}), flush=True)
result('## Result\n\nClaude-shaped invocation finished.\n')
"#;

/// The value a record written after this change carries.
/// §FS-rhei-cost-accounting.3.6
const STATED_CONVENTION: &str = "input-total-includes-cache";

/// The book both agents are priced from: `gpt-5.6-sol` at $4/M input and
/// $0.40/M cached read, `claude-sonnet-5` at $2/M and $0.20/M, both $20/M and
/// $10/M output. It is the real book that priced the archived records below.
fn shared_price_book() -> PathBuf {
    fixture_path("accounting-priced-archive/prices.json")
}

/// A workspace whose two states name two different mock agents.
fn mixed_workspace(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let codex = write_python_agent(&dir, "mock-codex.py", CODEX_AGENT);
    let claude = write_python_agent(&dir, "mock-claude.py", CLAUDE_AGENT);
    let settings_dir = dir.join(".agents/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "codex": {{ "command": {codex_command}, "prompt_flag": "--prompt", "timeout": "20s" }},
    "claude-code": {{
      "command": {claude_command},
      "prompt_flag": "--prompt",
      "timeout": "20s",
      "intervene_stdin": false
    }}
  }},
  "models": {{
    "sol": {{ "provider": "openai", "model": "gpt-5.6-sol", "default_agent": "codex" }},
    "sonnet": {{
      "provider": "anthropic",
      "model": "claude-sonnet-5",
      "default_agent": "claude-code"
    }}
  }}
}}"#,
            codex_command = fixture_command(&codex),
            claude_command = fixture_command(&claude),
        ),
    )
    .expect("write settings");
    let plan = write_fixture_file(&dir, "plan.rhei.md", MIXED_PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", MIXED_MACHINE);
    (dir, plan, machine)
}

/// Run both agents once, against the shared book, and return their records.
fn mixed_run(prefix: &str) -> (TestDir, PathBuf, PathBuf, Vec<serde_json::Value>) {
    let (dir, plan, machine) = mixed_workspace(prefix);
    let prices = shared_price_book().to_string_lossy().into_owned();
    let result =
        run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks", "--prices", &prices]);
    assert_success(&result);
    let records = invocation_records(&dir);
    assert_eq!(records.len(), 2, "two tasks, two agents, two records: {records:#?}");
    (dir, plan, machine, records)
}

fn record_for<'a>(records: &'a [serde_json::Value], agent: &str) -> &'a serde_json::Value {
    records
        .iter()
        .find(|record| record["agent"].as_str() == Some(agent))
        .unwrap_or_else(|| panic!("no record from {agent}; got:\n{records:#?}"))
}

fn dimension(record: &serde_json::Value, side: &str, name: &str) -> Option<u64> {
    record["tokens"][side][name]["value"].as_u64()
}

/// The ticket. Both records are `measured`, both satisfy
/// `total == input.total + output.total`, and `input.total` means a different
/// thing in each. The invariant is stated once, over every record, without
/// branching on which agent wrote it.
// §FS-rhei-cost-accounting.3.1
#[test]
fn a_mixed_run_writes_both_records_under_one_convention() {
    let (_dir, _plan, _machine, records) = mixed_run("convention-mixed");

    for record in &records {
        let agent = record["agent"].as_str().expect("record names its agent");
        let input_total =
            dimension(record, "input", "total").expect("a measured record has an input total");
        let cached_read = dimension(record, "input", "cached_read").unwrap_or(0);
        let cache_write = dimension(record, "input", "cache_write").unwrap_or(0);
        let output_total = record["tokens"]["output"]["total"]["value"].as_u64().expect("output");
        let total = record["tokens"]["total"]["value"].as_u64().expect("total");

        assert!(
            cached_read + cache_write <= input_total,
            "{agent}: {cached_read} cached + {cache_write} written of {input_total} input tokens"
        );
        assert_eq!(
            total,
            input_total + output_total,
            "{agent}: total is every token the invocation processed"
        );
    }

    // The provider's own arithmetic, converted. Codex already counts its cached
    // reads inside `input_tokens`; Claude's 100 fresh input tokens and 700
    // cached reads are 800 input tokens.
    let codex = record_for(&records, "codex");
    assert_eq!(dimension(codex, "input", "total"), Some(1_000));
    assert_eq!(dimension(codex, "input", "cached_read"), Some(700));
    let claude = record_for(&records, "claude-code");
    assert_eq!(dimension(claude, "input", "total"), Some(800), "100 fresh + 700 cached");
    assert_eq!(dimension(claude, "input", "cached_read"), Some(700));
}

/// The second defect. Both amounts are the reproduction's own: the `codex`
/// record was stored at 5,280 micro-USD where 2,480 was due, and the
/// `claude-code` record's amount must not move, because the same tokens are
/// charged at the same rates under either convention.
// §FS-rhei-cost-accounting.5
#[test]
fn no_cached_read_is_charged_twice_in_a_mixed_run() {
    let (_dir, _plan, _machine, records) = mixed_run("convention-priced");

    let codex = record_for(&records, "codex");
    assert_eq!(codex["pricing"]["status"], "priced");
    assert_eq!(
        codex["pricing"]["amount_micro"].as_u64(),
        Some(2_480),
        "300 fresh at $4/M, 700 cached at $0.40/M, 50 output at $20/M"
    );
    assert_eq!(codex["pricing"]["priced_amount_micro"].as_u64(), Some(2_480));

    let claude = record_for(&records, "claude-code");
    assert_eq!(claude["pricing"]["status"], "priced");
    assert_eq!(
        claude["pricing"]["amount_micro"].as_u64(),
        Some(840),
        "100 fresh at $2/M, 700 cached at $0.20/M, 50 output at $10/M"
    );
}

/// A reader of a mixed archive must never have to guess. The field is added the
/// way `run_id` was: optional, with the schema string standing still.
// §FS-rhei-cost-accounting.3.6 §FS-rhei-cost-accounting.8.1
#[test]
fn a_fresh_record_states_the_convention_it_follows() {
    let (dir, _plan, _machine, records) = mixed_run("convention-stated");

    for record in &records {
        let agent = record["agent"].as_str().expect("record names its agent");
        assert_eq!(
            record["schema"].as_str(),
            Some("rhei.accounting.invocation.v1"),
            "{agent}: the schema string does not move for an added optional field"
        );
        assert_eq!(
            record["token_convention"].as_str(),
            Some(STATED_CONVENTION),
            "{agent}: the record must say which convention it follows; got:\n{record:#?}"
        );
    }

    let published = super::rhei_command(dir.join("schema-home"))
        .args(["schema", "rhei.accounting.invocation.v1"])
        .output()
        .expect("schema command runs");
    assert!(published.status.success(), "{}", String::from_utf8_lossy(&published.stderr));
    let schema: serde_json::Value =
        serde_json::from_slice(&published.stdout).expect("published schema JSON");
    assert!(
        schema["properties"].get("token_convention").is_some(),
        "the published v1 schema must declare the optional field; got:\n{:#?}",
        schema["properties"]
    );
}

// Two invocation records this machine really wrote on 2026-09-04, copied out of
// `~/f/.accounting-archive/`, with the price book that priced them. Neither
// carries `token_convention`, because it did not exist. They are the population
// the fix has to reach: 997 such records were archived that day, 274 of them
// `codex` carrying 627M tokens.
//
// codex, gpt-5.6-sol: 115,497 input of which 94,208 cached, 2,188 output.
//   stored 543,431 micro-USD -- 115,497 charged at $4/M and 94,208 again at
//   $0.40/M. Due: 21,289 fresh at $4/M + 94,208 at $0.40/M + 2,188 at $20/M.
const CODEX_CORRECTED_MICRO: u64 = 166_599;
const CODEX_STORED_MICRO: u64 = 543_431;
const CODEX_TOTAL_TOKENS: u64 = 117_685;
// claude-code, claude-sonnet-5: 18 fresh input, 550,496 cached, 52,423 written,
//   3,508 output. Its money is already right, because its dimensions were
//   disjoint and were priced as disjoint. Its token totals were not: stored
//   `total` is 3,526 where 602,937 input plus 3,508 output were processed.
const CLAUDE_STORED_MICRO: u64 = 276_272;
const CLAUDE_RESTATED_TOTAL_TOKENS: u64 = 606_445;

/// What the two records cost when each is read under its own agent's
/// convention, and what they cost when the `codex` one is taken at face value.
const ARCHIVE_CORRECTED_MICRO: u64 = CODEX_CORRECTED_MICRO + CLAUDE_STORED_MICRO;
const ARCHIVE_FACE_VALUE_MICRO: u64 = CODEX_STORED_MICRO + CLAUDE_STORED_MICRO;
const ARCHIVE_TOTAL_TOKENS: u64 = CODEX_TOTAL_TOKENS + CLAUDE_RESTATED_TOTAL_TOKENS;

/// Copy the two priced records into a workspace, with or without the book that
/// priced them beside them. Without it, the book is named by id alone and is
/// unreachable. §FS-rhei-cost-accounting.5.2
fn seed_priced_archive(workspace_root: &Path, with_price_book: bool) {
    let accounting = workspace_root.join("runtime/accounting");
    let invocations = accounting.join("invocations");
    fs::create_dir_all(&invocations).expect("create invocations dir");
    for name in ["codex-priced.json", "claude-code-priced.json"] {
        fs::copy(fixture_path("accounting-priced-archive").join(name), invocations.join(name))
            .expect("copy archived record");
    }
    if with_price_book {
        fs::copy(shared_price_book(), accounting.join("prices.json")).expect("copy price book");
    }
}

fn cost_json(plan: &Path, machine: &Path) -> serde_json::Value {
    let result = run_cli("cost", plan, machine, &["--json"]);
    assert_success(&result);
    serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("cost --json should emit JSON ({err}); got:\n{}", result.stdout)
    })
}

/// The half of the ticket that decides whether the money is actually fixed.
/// `rhei cost --run`, `--by`, and its windowed totals all recompute from stored
/// records, so a `codex` record written before the convention existed keeps
/// reporting its double charge unless the reading infers the convention from
/// the record's own agent.
// §FS-rhei-cost-accounting.5.2
#[test]
fn a_stored_record_is_read_under_the_convention_its_agent_implies() {
    let (dir, plan, machine) = accounting_workspace("convention-archive", TERMINAL_PLAN);
    seed_priced_archive(&dir, true);
    let before: Vec<String> = ["codex-priced.json", "claude-code-priced.json"]
        .iter()
        .map(|name| {
            fs::read_to_string(dir.join("runtime/accounting/invocations").join(name))
                .expect("archived record seeded")
        })
        .collect();

    let payload = cost_json(&plan, &machine);
    let summary = &payload["summary"];

    assert_eq!(
        summary["cost_micro"].as_u64(),
        Some(ARCHIVE_CORRECTED_MICRO),
        "the codex record's cached reads are charged once; got:\n{summary:#?}"
    );
    assert_ne!(
        summary["cost_micro"].as_u64(),
        Some(ARCHIVE_FACE_VALUE_MICRO),
        "reading the stored amount at face value carries the double charge forward"
    );
    assert_eq!(
        summary["total"]["value"].as_u64(),
        Some(ARCHIVE_TOTAL_TOKENS),
        "the claude-code record's cached reads are tokens it processed; got:\n{summary:#?}"
    );

    // Stored amounts are a record of what was computed, and stay as written.
    for (name, text) in ["codex-priced.json", "claude-code-priced.json"].iter().zip(before) {
        let after = fs::read_to_string(dir.join("runtime/accounting/invocations").join(name))
            .expect("archived record still there");
        assert_eq!(after, text, "{name} is history, not something a reading rewrites");
    }
}

/// The seam the archive actually presents. 514 of the 997 archived records name
/// `foundation-site-2026-09-02`, a caller-owned book that is not on disk beside
/// them, so "price it correctly" cannot always mean "recompute from its book".
/// The reading says what it could not stand behind rather than carrying an
/// amount it knows to be an over-charge.
// §FS-rhei-cost-accounting.5.2 §FS-rhei-cost-accounting.6.2
#[test]
fn a_record_whose_price_book_is_unreachable_is_read_unpriced() {
    let (dir, plan, machine) = accounting_workspace("convention-no-book", TERMINAL_PLAN);
    seed_priced_archive(&dir, false);
    assert!(
        !dir.join("runtime/accounting/prices.json").exists(),
        "the point of this workspace is that the named book is not beside the records"
    );

    let payload = cost_json(&plan, &machine);
    let summary = &payload["summary"];

    assert_eq!(
        summary["cost_micro"].as_u64(),
        None,
        "one record could not be priced, so there is no whole amount; got:\n{summary:#?}"
    );
    assert_eq!(
        summary["priced_cost_micro"].as_u64(),
        Some(CLAUDE_STORED_MICRO),
        "the lower bound is the record whose money needed no correction"
    );
    assert_eq!(summary["pricing_status"].as_str(), Some("partial-price"));
    assert_eq!(summary["coverage"].as_str(), Some("partial"));
    assert_eq!(
        summary["total"]["value"].as_u64(),
        Some(ARCHIVE_TOTAL_TOKENS),
        "restating tokens never needed the book, so the tokens still count"
    );
}
