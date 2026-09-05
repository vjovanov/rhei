// §AR-source-file-size.3: the arithmetic of selecting and rolling up
// invocation records.

// Where a window's edges fall, and what an aggregate is allowed to claim about
// a set it could not fully see. The CLI surface those answers reach an operator
// through is in the `cost_*` end-to-end scenarios.

/// One invocation record, written straight into a workspace's accounting
/// directory. Every field the reader needs and nothing it does not, so a test
/// can put a record at an exact instant. §FS-rhei-cost-accounting.3
fn write_invocation_record(
    workspace: &Path,
    file_stem: &str,
    run_id: Option<&str>,
    started_at: &str,
    total_tokens: u64,
) {
    let dir = workspace.join("runtime/accounting/invocations");
    fs::create_dir_all(&dir).expect("create invocations dir");
    let mut record = serde_json::json!({
        "schema": "rhei.accounting.invocation.v1",
        "invocation_id": format!("plan.1::work::mock::{file_stem}"),
        "task_id": "plan.1",
        "state": "work",
        "visit": 1,
        "agent": "claude-code",
        "provider": "anthropic",
        "model": "claude-sonnet-4-6",
        "started_at": started_at,
        "ended_at": started_at,
        "extraction_status": "measured",
        "scope": "aggregate-agent-process",
        "tokens": {
            "total": { "value": total_tokens, "source": "agent-usage-capture" },
            "input": {
                "total": { "value": total_tokens, "source": "agent-usage-capture" },
                "cached_read": { "status": "unsupported" },
                "cache_write": { "status": "unsupported" }
            },
            "output": {
                "total": { "value": 0, "source": "agent-usage-capture" },
                "cached_read": { "status": "unsupported" },
                "cache_write": { "status": "unsupported" }
            }
        },
        "pricing": {
            "status": "priced",
            "currency": "USD",
            "amount_micro": 1,
            "price_book_id": "builtin-2026-05-20"
        }
    });
    if let Some(run_id) = run_id {
        record["run_id"] = serde_json::Value::String(run_id.to_string());
    }
    fs::write(
        dir.join(format!("{file_stem}.json")),
        serde_json::to_string_pretty(&record).expect("record serializes"),
    )
    .expect("write invocation record");
}

/// A workspace whose plan is one terminal ticket: enough for `rhei cost` to
/// load, and nothing that would write records of its own.
fn accounting_probe_workspace(prefix: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Accounting Arithmetic\n\n## Tasks\n\n### Task 1: Done\n**State:** completed\n",
    );
    let machine_path = write_fixture_file(
        &dir,
        "states.yaml",
        "name: accounting-arithmetic\nversion: 1\nstates:\n  work:\n    initial: true\n  \
         completed:\n    final: true\ntransitions:\n  - from: work\n    to: completed\n",
    );
    (dir, plan_path, machine_path)
}

fn cost_payload(plan: &Path, machine: &Path, args: &[&str]) -> serde_json::Value {
    let mut cmd = rhei_command();
    cmd.arg("--state-machine").arg(machine).arg("cost").arg(plan).arg("--json");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei cost should run");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "rhei cost {args:?} should succeed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("cost --json should emit JSON ({err}); got:\n{stdout}"))
}

/// `[since, until)`. A record at the lower bound is in; one at the upper bound
/// is the next window's. Without that, two adjacent windows either double-count
/// a record or lose it.
// §FS-rhei-cost-accounting.6.1
#[test]
fn a_window_on_started_at_is_half_open() {
    let (dir, plan_path, machine_path) = accounting_probe_workspace("accounting-window");
    write_invocation_record(&dir, "before", None, "2026-09-01T05:06:57Z", 7);
    write_invocation_record(&dir, "lower-bound", None, "2026-09-01T05:06:58Z", 100);
    write_invocation_record(&dir, "inside", None, "2026-09-01T05:07:00Z", 20);
    write_invocation_record(&dir, "upper-bound", None, "2026-09-01T05:07:40Z", 3000);

    let payload = cost_payload(
        &plan_path,
        &machine_path,
        &["--since", "2026-09-01T05:06:58Z", "--until", "2026-09-01T05:07:40Z"],
    );
    assert_eq!(
        payload["selection"]["invocation_count"].as_u64(),
        Some(2),
        "the lower bound is in and the upper bound is out; got:\n{payload:#?}"
    );
    assert_eq!(
        payload["summary"]["total"]["value"].as_u64(),
        Some(120),
        "the window totals exactly the records inside it; got:\n{payload:#?}"
    );
}

/// The rule the whole ticket turns on: an aggregate that could not see every
/// record it should have must not read as if it did.
// §FS-rhei-cost-accounting.6.2
#[test]
fn a_run_selection_stops_reporting_complete_once_a_record_names_no_run() {
    let (dir, plan_path, machine_path) = accounting_probe_workspace("accounting-coverage");
    write_invocation_record(&dir, "mine-1", Some("aaa111"), "2026-09-01T05:06:58Z", 100);
    write_invocation_record(&dir, "mine-2", Some("aaa111"), "2026-09-01T05:07:00Z", 20);
    write_invocation_record(&dir, "theirs", Some("bbb222"), "2026-09-01T05:07:40Z", 3000);

    let clean = cost_payload(&plan_path, &machine_path, &["--run", "aaa111"]);
    assert_eq!(clean["summary"]["total"]["value"].as_u64(), Some(120));
    assert_eq!(
        clean["summary"]["coverage"].as_str(),
        Some("complete"),
        "every record names a run, so nothing is unaccounted for; got:\n{clean:#?}"
    );

    write_invocation_record(&dir, "nobodys", None, "2026-09-01T05:08:00Z", 9);
    let mixed = cost_payload(&plan_path, &machine_path, &["--run", "aaa111"]);
    assert_eq!(
        mixed["summary"]["total"]["value"].as_u64(),
        Some(120),
        "the total is still only what named this run"
    );
    assert_eq!(
        mixed["summary"]["coverage"].as_str(),
        Some("partial"),
        "one record could belong to this run and cannot say so; got:\n{mixed:#?}"
    );
    assert_eq!(
        mixed["run_attribution"]["unattributed_invocation_count"].as_u64(),
        Some(1),
        "and the count of what could not be attributed is on the answer"
    );
}

/// A grouping partitions the selection: nothing selected may fall outside every
/// group, whatever its `run_id` says or does not say.
// §FS-rhei-cost-accounting.6.1
#[test]
fn grouping_by_run_partitions_the_selection_without_losing_a_record() {
    let (dir, plan_path, machine_path) = accounting_probe_workspace("accounting-partition");
    write_invocation_record(&dir, "mine", Some("aaa111"), "2026-09-01T05:06:58Z", 100);
    write_invocation_record(&dir, "theirs", Some("bbb222"), "2026-09-01T05:07:40Z", 3000);
    write_invocation_record(&dir, "nobodys-1", None, "2026-09-01T05:08:00Z", 9);
    write_invocation_record(&dir, "nobodys-2", None, "2026-09-01T05:09:00Z", 11);

    let payload = cost_payload(&plan_path, &machine_path, &["--by", "run"]);
    let groups = payload["groups"].as_array().expect("groups is an array");
    let grouped: u64 = groups
        .iter()
        .map(|entry| entry["summary"]["invocation_count"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(
        grouped,
        payload["selection"]["invocation_count"].as_u64().unwrap_or(0),
        "every selected record lands in exactly one group; got:\n{payload:#?}"
    );

    let unattributed = groups
        .iter()
        .find(|entry| entry["unattributed"].as_bool() == Some(true))
        .unwrap_or_else(|| panic!("no unattributed group; got:\n{:#?}", payload["groups"]));
    assert_eq!(unattributed["key"].as_str(), Some("(unattributed)"));
    assert_eq!(unattributed["summary"]["total"]["value"].as_u64(), Some(20));
}
