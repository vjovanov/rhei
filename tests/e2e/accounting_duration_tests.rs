// §AR-source-file-size.3

//! How long an invocation took, as `rhei cost --json --task` publishes it.
//!
//! The archive `accounting_support` seeds is six records this machine really
//! wrote before `duration_ms` existed: each one carries `started_at` and
//! `ended_at` and no stored duration. That is the shape behind
//! `agent-grounds/rhei#190`, where 1,558 minutes of wall clock were published
//! as nothing at all. The field stays optional under
//! §FS-rhei-cost-accounting.8.1, so what is asserted here is the number a
//! reader reports, never the shape of a record on disk.
//! §FS-rhei-cost-accounting.3.4.1

use std::fs;
use std::path::Path;

use super::accounting_support::{
    accounting_workspace, invocation_records, seed_archived_records, WORKING_PLAN,
};
use super::{assert_success, run_cli, CliRun};

/// One archived subtree, and what its two records' endpoints say they took:
/// `06:20:07Z -> 06:30:42Z` is 10m35s and `07:16:42Z -> 07:45:18Z` is 28m36s.
/// Both were written by `claude-code` before the field existed.
const MEASURED_SUBTREE: [(&str, &str, &str, u64); 2] = [
    (
        "github-issues-vjovanov-rhei-138-fix-issue.ticket",
        "2026-09-02T06:20:07Z",
        "2026-09-02T06:30:42Z",
        635_000,
    ),
    (
        "github-issues-vjovanov-rhei-138-fix-issue.ticket.implement",
        "2026-09-02T07:16:42Z",
        "2026-09-02T07:45:18Z",
        1_716_000,
    ),
];

/// The other archived subtree. Its `.review-1` record is the set's
/// `no-usage-emitted` one: nothing measured a token on it, and it still ran for
/// 18 seconds. `05:06:58Z -> 05:07:40Z` is 42s; `05:07:40Z -> 05:07:58Z` is 18s.
const UNMEASURED_SUBTREE: [(&str, u64); 2] = [
    ("github-issues-vjovanov-ephor-14-fix-issue.ticket", 42_000),
    ("github-issues-vjovanov-ephor-14-fix-issue.ticket.review-1", 18_000),
];

fn cost_json(plan: &Path, machine: &Path, args: &[&str]) -> serde_json::Value {
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let result: CliRun = run_cli("cost", plan, machine, &with_json);
    assert_success(&result);
    serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
        panic!("cost --json should emit JSON ({err}); got:\n{}", result.stdout)
    })
}

/// The published invocations of one task, keyed by the node each is charged to.
fn published_invocations(payload: &serde_json::Value) -> Vec<serde_json::Value> {
    payload["task"]["invocations"]
        .as_array()
        .unwrap_or_else(|| {
            panic!("cost --json --task publishes an invocations array; got:\n{payload:#?}")
        })
        .clone()
}

fn invocation_of<'a>(invocations: &'a [serde_json::Value], task_id: &str) -> &'a serde_json::Value {
    invocations.iter().find(|record| record["task_id"].as_str() == Some(task_id)).unwrap_or_else(
        || panic!("no published invocation charged to {task_id}; got:\n{invocations:#?}"),
    )
}

/// The ticket itself. A record written before `duration_ms` existed carries
/// both endpoints, so the elapsed time between them is known; publishing it as
/// nothing is what made a 28m36s invocation count as zero.
// §FS-rhei-cost-accounting.3.4.1
#[test]
fn cost_json_reports_the_elapsed_time_of_a_record_written_before_duration_ms() {
    let (dir, plan_path, machine_path) =
        accounting_workspace("cost-duration-derived", WORKING_PLAN);
    seed_archived_records(&dir);
    let archived = dir.join("runtime/accounting/invocations/claude-code-3.json");
    let before = fs::read_to_string(&archived).expect("archived record seeded");

    let payload = cost_json(&plan_path, &machine_path, &["--task", MEASURED_SUBTREE[0].0]);
    let invocations = published_invocations(&payload);
    assert_eq!(invocations.len(), 2, "the node and its child; got:\n{invocations:#?}");

    for (task_id, started_at, ended_at, elapsed_ms) in MEASURED_SUBTREE {
        let record = invocation_of(&invocations, task_id);
        assert_eq!(record["started_at"].as_str(), Some(started_at));
        assert_eq!(record["ended_at"].as_str(), Some(ended_at));
        assert_eq!(
            record["duration_ms"].as_u64(),
            Some(elapsed_ms),
            "{task_id} ran from {started_at} to {ended_at}: the reading reports the \
             {elapsed_ms} ms between them, not {}",
            record["duration_ms"]
        );
    }

    // The number is reported, and the record is still the record: a reading
    // recomputes what it reports and never rewrites what it read.
    // §FS-rhei-cost-accounting.5.1
    let after = fs::read_to_string(&archived).expect("archived record still there");
    assert_eq!(before, after, "reading an archive does not write to it");
    let stored: serde_json::Value = serde_json::from_str(&after).expect("archived record parses");
    assert!(
        stored.get("duration_ms").is_none(),
        "the stored record keeps no duration; the field is optional in v1 and stays absent"
    );
}

/// Timing does not depend on token extraction. The one archived record nothing
/// could measure ran for 18 seconds, and that is as recoverable as any other.
// §FS-rhei-cost-accounting.3.4.1
#[test]
fn an_archived_invocation_that_measured_no_tokens_still_reports_what_it_took() {
    let (dir, plan_path, machine_path) =
        accounting_workspace("cost-duration-unmeasured", WORKING_PLAN);
    seed_archived_records(&dir);

    let payload = cost_json(&plan_path, &machine_path, &["--task", UNMEASURED_SUBTREE[0].0]);
    let invocations = published_invocations(&payload);

    for (task_id, elapsed_ms) in UNMEASURED_SUBTREE {
        let record = invocation_of(&invocations, task_id);
        assert_eq!(
            record["duration_ms"].as_u64(),
            Some(elapsed_ms),
            "{task_id} ran {elapsed_ms} ms between {} and {}, reported as {}",
            record["started_at"],
            record["ended_at"],
            record["duration_ms"]
        );
    }
    assert_eq!(
        invocation_of(&invocations, UNMEASURED_SUBTREE[1].0)["extraction_status"].as_str(),
        Some("no-usage-emitted"),
        "the fixture this asserts on is the one that measured nothing"
    );
}

/// The other side of the rule: a record that carries its own `duration_ms` is
/// published with that number. It was measured while the agent ran and is finer
/// than the second-precision endpoints, so a reading that derived over it would
/// round a 31 ms invocation down to zero.
// §FS-rhei-cost-accounting.3.4.1
#[test]
fn a_stored_duration_is_published_exactly_as_the_record_wrote_it() {
    let (dir, plan_path, machine_path) = accounting_workspace("cost-duration-stored", WORKING_PLAN);
    assert_success(&run_cli("run", &plan_path, &machine_path, &["--no-tui", "--no-callbacks"]));

    let written = invocation_records(&dir);
    assert_eq!(written.len(), 1, "one agent ran, so one record: {written:#?}");
    let record = &written[0];
    let stored = record["duration_ms"]
        .as_u64()
        .unwrap_or_else(|| panic!("this binary writes a duration; got:\n{record:#?}"));

    let task_id = record["task_id"].as_str().expect("the record names its task");
    let payload = cost_json(&plan_path, &machine_path, &["--task", task_id]);
    let invocations = published_invocations(&payload);
    assert_eq!(
        invocation_of(&invocations, task_id)["duration_ms"].as_u64(),
        Some(stored),
        "a measured duration is published as written, not recomputed from the endpoints"
    );
}
