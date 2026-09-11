//! The Panta project the scope scenarios of `agent-grounds/rhei#188` drive,
//! and the numbers they read the answer off.
//!
//! One project holds every shape a reading has to get right at once: a
//! Directory Workspace member with records of its own, two single-file rheis
//! whose execution root *is* the project directory and so is shared, a `basin`
//! with records, and a member that has never been run and holds no accounting
//! directory at all.
//! [§FS-rhei-panta.6.5](../../docs/functional-spec/rhei-panta.spec.md#65-cost-and-summary)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{rhei_command, unique_temp_dir, write_fixture_file, CliRun, TestDir, STATE_MACHINE};

/// `tokens.total` on each record, in powers of two so that **every subset of
/// the roots sums to a number no other subset can produce**. A reading that
/// totals 30,000 read the project directory and nothing else; one that totals
/// 7,000 read the member. That is what makes an assertion on a total an
/// assertion about *which roots were searched* rather than about arithmetic.
pub const BILLING_RECORD_TOKENS: [u64; 3] = [1_000, 2_000, 4_000];
pub const LEDGER_RECORD_TOKENS: u64 = 10_000;
pub const SPOOL_RECORD_TOKENS: u64 = 20_000;
pub const BASIN_RECORD_TOKENS: u64 = 40_000;

/// What the member holds on its own: three records under its own root.
pub const BILLING_TOKENS: u64 = 7_000;
pub const BILLING_RECORDS: u64 = 3;

/// What the project holds altogether: the member, the shared project directory
/// the two single-file rheis root at, and the basin. The shared root is read
/// **once** — counted twice it would total 107,000 over 8 records.
pub const PROJECT_TOKENS: u64 = 77_000;
pub const PROJECT_RECORDS: u64 = 6;

/// The accounting roots a project-wide reading searches: the project directory
/// (the run root, and the two single-file rheis' root, deduplicated to one),
/// `billing`, `quiet`, and `basin`.
pub const PROJECT_ROOTS: u64 = 4;

/// A price book both fixture models resolve in, so a record stays `priced` and
/// the reading's coverage is `Complete`. Every accounting root gets a copy, as
/// a run leaves one in each participating root.
/// [§FS-rhei-cost-accounting.5.1](../../docs/functional-spec/rhei-cost-accounting.spec.md#51-price-book-selection)
const PRICE_BOOK: &str = r#"{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "scope-fixture",
  "currency": "USD",
  "entries": [
    { "provider": "anthropic", "model": "claude-sonnet-4-6",
      "effective_at": "2026-01-01T00:00:00Z", "unit": "1m_tokens",
      "input_total_micro": 3000000, "input_cached_read_micro": 300000,
      "input_cache_write_micro": 3750000, "output_total_micro": 15000000 },
    { "provider": "anthropic", "model": "claude-opus-5",
      "effective_at": "2026-01-01T00:00:00Z", "unit": "1m_tokens",
      "input_total_micro": 5000000, "input_cached_read_micro": 500000,
      "input_cache_write_micro": 6250000, "output_total_micro": 25000000 }
  ]
}"#;

/// The fixture project, and the paths a scenario addresses it by.
pub struct ScopeFixture {
    /// Bound so the tree outlives the test.
    pub dir: TestDir,
    pub project: PathBuf,
    pub machine: PathBuf,
    pub home: PathBuf,
}

impl ScopeFixture {
    pub fn member(&self, id: &str) -> PathBuf {
        self.project.join(id)
    }
}

/// One `rhei.accounting.invocation.v1` record with no cache dimensions, so
/// §FS-rhei-cost-accounting.5.2 needs no recomputation and the record stays
/// priced whichever root it is read from.
fn write_record(root: &Path, id: &str, task_id: &str, model: &str, minute: u64, total: u64) {
    let dir = root.join("runtime/accounting/invocations");
    fs::create_dir_all(&dir).expect("create invocations directory");
    let input = total * 4 / 5;
    let output = total - input;
    let record = format!(
        r#"{{
  "schema": "rhei.accounting.invocation.v1",
  "invocation_id": "{id}",
  "task_id": "{task_id}",
  "state": "draft",
  "visit": 1,
  "agent": "claude-code",
  "provider": "anthropic",
  "model": "{model}",
  "started_at": "2026-09-01T10:{minute:02}:00Z",
  "ended_at": "2026-09-01T10:{minute:02}:30Z",
  "duration_ms": 30000,
  "extraction_status": "measured",
  "scope": "aggregate-agent-process",
  "tokens": {{
    "total": {{ "value": {total}, "source": "agent-usage-capture" }},
    "input": {{
      "total": {{ "value": {input}, "source": "agent-usage-capture" }},
      "cached_read": {{ "status": "unsupported" }},
      "cache_write": {{ "status": "unsupported" }}
    }},
    "output": {{
      "total": {{ "value": {output}, "source": "agent-usage-capture" }},
      "cached_read": {{ "status": "unsupported" }},
      "cache_write": {{ "status": "unsupported" }}
    }}
  }},
  "pricing": {{ "status": "priced", "currency": "USD", "amount_micro": 1000,
                "priced_amount_micro": 1000, "price_book_id": "scope-fixture" }}
}}"#
    );
    fs::write(dir.join(format!("{id}.json")), record).expect("write invocation record");
    fs::write(root.join("runtime/accounting/prices.json"), PRICE_BOOK).expect("write price book");
}

/// A Directory Workspace member with the task files it is given.
fn write_member(project: &Path, id: &str, tasks: &str) {
    let root = project.join(id);
    fs::create_dir_all(root.join("tasks")).expect("create member workspace");
    fs::write(root.join("index.rhei.md"), format!("# Rhei: {id}\n")).expect("write member index");
    fs::write(root.join("tasks/01-work.md"), tasks).expect("write member tasks");
}

/// The project every scenario but the empty one and the standalone guard runs
/// against. Its tasks are spread across terminal and non-terminal states so
/// that `rhei summary`'s tally reads differently for the member than for the
/// project — a tally that did not narrow with the records would show it.
pub fn scope_fixture(prefix: &str) -> ScopeFixture {
    let fixture = empty_scope_fixture(prefix);
    let project = &fixture.project;
    for (index, total) in BILLING_RECORD_TOKENS.iter().enumerate() {
        write_record(
            &project.join("billing"),
            &format!("billing-{index}"),
            if index == 2 { "billing.2" } else { "billing.1" },
            "claude-sonnet-4-6",
            index as u64,
            *total,
        );
    }
    // The two single-file rheis root at the project directory itself, so these
    // two records and the run root's own tree are one directory.
    // §AR-rhei-panta.5
    write_record(project, "ledger-0", "ledger.1", "claude-opus-5", 10, LEDGER_RECORD_TOKENS);
    write_record(project, "spool-0", "spool.1", "claude-opus-5", 11, SPOOL_RECORD_TOKENS);
    write_record(
        &project.join("basin"),
        "basin-0",
        "basin.1",
        "claude-opus-5",
        12,
        BASIN_RECORD_TOKENS,
    );
    fixture
}

/// The same project with no accounting anywhere in it: the shape scenario 6
/// needs, where the answer is empty and the only useful thing to say is where
/// the command looked.
pub fn empty_scope_fixture(prefix: &str) -> ScopeFixture {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");
    fs::write(project.join("index.panta.md"), "# Panta: Scope Fixture\n")
        .expect("write project manifest");
    fs::write(
        project.join("ledger.rhei.md"),
        "# Rhei: Ledger\n\n## Tasks\n\n### Task 1: Post the entries\n**State:** draft\n",
    )
    .expect("write single-file rhei");
    fs::write(
        project.join("spool.rhei.md"),
        "# Rhei: Spool\n\n## Tasks\n\n### Task 1: Spool it\n**State:** cancelled\n",
    )
    .expect("write second single-file rhei");
    write_member(
        &project,
        "billing",
        "### Task 1: Bill it\n**State:** completed\n\n### Task 2: Bill it again\n**State:** draft\n",
    );
    // A member that has never been run: no `runtime/` at all, so its accounting
    // root does not exist. It contributes nothing and is not an error.
    write_member(&project, "quiet", "### Task 1: Say nothing\n**State:** draft\n");
    fs::create_dir_all(project.join("basin")).expect("create basin");
    fs::write(project.join("basin/01-unfiled.md"), "### Task 1: Unfiled\n**State:** draft\n")
        .expect("write basin ticket");
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let home = dir.join(".home");
    ScopeFixture { dir, project, machine, home }
}

/// A standalone Directory Workspace with one record: no `index.panta.md`
/// anywhere above it, so it is the control whose output must not move.
pub fn standalone_fixture(prefix: &str) -> ScopeFixture {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("solo");
    fs::create_dir_all(project.join("tasks")).expect("create standalone workspace");
    fs::write(project.join("index.rhei.md"), "# Rhei: Solo\n").expect("write index");
    fs::write(project.join("tasks/01-work.md"), "### Task 1: Work alone\n**State:** draft\n")
        .expect("write task");
    write_record(&project, "solo-0", "solo.1", "claude-sonnet-4-6", 0, LEDGER_RECORD_TOKENS);
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let home = dir.join(".home");
    ScopeFixture { dir, project, machine, home }
}

/// Run one rhei invocation *from inside* `cwd`, passing the argv verbatim. The
/// spellings under test only exist relative to an invocation directory, so
/// nothing here may resolve a path on the test's behalf.
pub fn rhei_from(fixture: &ScopeFixture, cwd: &Path, argv: &[&str]) -> CliRun {
    let mut cmd: Command = rhei_command(&fixture.home);
    cmd.current_dir(cwd);
    cmd.arg("--state-machine").arg(&fixture.machine);
    for arg in argv {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `rhei cost --json` over a target, parsed. §FS-rhei-cost-accounting.8.4
pub fn cost_json(fixture: &ScopeFixture, target: &str, extra: &[&str]) -> serde_json::Value {
    let mut argv = vec!["cost", target];
    argv.extend_from_slice(extra);
    argv.push("--json");
    let result = rhei_from(fixture, &fixture.dir, &argv);
    assert!(
        result.status.success(),
        "`rhei {}` should succeed\nstdout:\n{}\nstderr:\n{}",
        argv.join(" "),
        result.stdout,
        result.stderr
    );
    serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("cost --json should emit JSON ({err}); got:\n{}", result.stdout)
    })
}

/// `tokens.total` of a cost payload's workspace summary.
pub fn payload_tokens(payload: &serde_json::Value) -> u64 {
    payload["summary"]["total"]["value"]
        .as_u64()
        .unwrap_or_else(|| panic!("the summary should carry a measured total; got:\n{payload:#?}"))
}

/// How many records the payload says it computed over.
pub fn payload_records(payload: &serde_json::Value) -> u64 {
    payload["selection"]["invocation_count"]
        .as_u64()
        .unwrap_or_else(|| panic!("the selection should carry a count; got:\n{payload:#?}"))
}

/// The numbers an assertion on a cost payload turns on, on one line. A failure
/// here should read as a wrong reading, not as a JSON document.
pub fn digest(payload: &serde_json::Value) -> String {
    format!(
        "invocation_count={} total={} roots={}",
        payload["selection"]["invocation_count"],
        payload["summary"]["total"]["value"],
        payload["roots"]
    )
}
