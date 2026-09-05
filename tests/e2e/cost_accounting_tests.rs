// §AR-source-file-size.3

// What a run's own accounting says about that run: the record names the run
// that produced it, and the report's strip reports that run rather than
// everything the workspace has ever spent.

use std::fs;

use super::accounting_support::{
    accounting_workspace, invocation_records, last_run_id, run_report, seed_archived_records,
    ARCHIVED_RECORDS, TERMINAL_PLAN, WORKING_PLAN,
};
use super::{assert_success, run_cli};

/// The root of everything else in the ticket: nothing on a usage record says
/// which invocation of `rhei run` produced it, so attributing spend to a run is
/// an inference from timestamps rather than a fact on the record.
// §FS-rhei-cost-accounting.3.5
#[test]
fn a_fresh_invocation_record_names_the_run_that_produced_it() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-run-id", WORKING_PLAN);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let records = invocation_records(&dir);
    assert_eq!(records.len(), 1, "one agent ran, so one record: {records:#?}");
    let record = &records[0];
    assert_eq!(
        record["schema"].as_str(),
        Some("rhei.accounting.invocation.v1"),
        "the schema string does not move: an older record must still parse"
    );
    assert_eq!(
        record["run_id"].as_str(),
        Some(last_run_id(&dir).as_str()),
        "the record must name the run that produced it; got:\n{record:#?}"
    );
}

/// The ticket's sharpest symptom. A workspace that has been worked in holds
/// records from every run before this one; a run that spawns nothing must not
/// print their sum as what it spent.
// §FS-rhei-run-report.2.1
#[test]
fn a_run_that_spawned_no_agent_does_not_report_the_workspace_lifetime_total() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-strip-zero", TERMINAL_PLAN);
    seed_archived_records(&dir);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let report = run_report(&dir);
    assert!(
        report.contains("| agent invocations | 0 |"),
        "this run was meant to spawn nothing; got:\n{report}"
    );
    assert!(
        report.contains("| Accounting (this run) | Value |"),
        "the strip must name its scope; got:\n{report}"
    );
    assert!(
        report.contains("| total tokens | 0 |"),
        "a run that spawned no agent spent nothing; got:\n{report}"
    );
    assert!(
        !report.contains("| total tokens | 1.7M |"),
        "the workspace's lifetime total is not this run's cost; got:\n{report}"
    );
    assert!(
        report.contains("| workspace total tokens | 1.7M |"),
        "the lifetime total keeps a labelled row of its own; got:\n{report}"
    );
}

/// The other half of the same rule: when the run did spend something, the strip
/// reports that, says which of its two sources produced it, and keeps the
/// workspace's lifetime figure separate.
// §FS-rhei-run-report.2.1
#[test]
fn the_strip_reports_this_runs_tokens_beside_the_workspace_total() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-strip-run", WORKING_PLAN);
    seed_archived_records(&dir);

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    assert_eq!(
        invocation_records(&dir).len() as u64,
        ARCHIVED_RECORDS + 1,
        "the archived records stay, and the run adds one of its own"
    );

    let report = run_report(&dir);
    assert!(
        report.contains("| source | rollup |"),
        "the strip must say which quantity it is; got:\n{report}"
    );
    assert!(
        report.contains("| total tokens | 4.3k |"),
        "this run spent what its one agent reported; got:\n{report}"
    );
    assert!(
        report.contains("| workspace total tokens | 1.7M |"),
        "the workspace lifetime total keeps its own row; got:\n{report}"
    );
}

/// Records that name no run are the workspace's whole history before this
/// change, and a run must not adopt them by writing over them.
// §FS-rhei-cost-accounting.3.5
#[test]
fn a_run_leaves_the_records_that_name_no_run_exactly_as_it_found_them() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-archive-intact", WORKING_PLAN);
    seed_archived_records(&dir);
    let before = fs::read_to_string(dir.join("runtime/accounting/invocations/codex-1.json"))
        .expect("archived record seeded");

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let after = fs::read_to_string(dir.join("runtime/accounting/invocations/codex-1.json"))
        .expect("archived record still there");
    assert_eq!(before, after, "an archived record is history, not something a run rewrites");

    let unattributed = invocation_records(&dir)
        .into_iter()
        .filter(|record| record.get("run_id").is_none_or(serde_json::Value::is_null))
        .count() as u64;
    assert_eq!(
        unattributed, ARCHIVED_RECORDS,
        "the six archived records still name no run, and are still readable"
    );
}
