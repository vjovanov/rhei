// §AR-source-file-size.3

// What `rhei cost` and `rhei summary` read when the target is a Panta project
// or one of its members. A run laid into a project writes each record under
// the execution root of the rhei that owns the ticket, and both commands read
// the root above it instead, so a member that holds seventeen records reports
// none (agent-grounds/rhei#188).

// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.8

use super::cost_project_scope_support::{
    cost_json, digest, empty_scope_fixture, payload_records, payload_tokens, rhei_from,
    scope_fixture, standalone_fixture, BILLING_RECORDS, BILLING_TOKENS, LEDGER_RECORD_TOKENS,
    PROJECT_RECORDS, PROJECT_ROOTS, PROJECT_TOKENS,
};
use super::{assert_success, CliRun};

/// Everything the command said, so an assertion that fails shows both streams.
fn said(result: &CliRun) -> String {
    format!("stdout:\n{}\nstderr:\n{}", result.stdout, result.stderr)
}

/// Scenario 1 — Every spelling of a member names the same rhei (§FS-rhei-panta.6), so
/// every spelling must report that member's records — and agree, because a
/// reading that depends on how the path was typed is not a reading of anything.
// §FS-rhei-panta.6.5
#[test]
fn every_member_spelling_reports_that_members_own_records() {
    let fixture = scope_fixture("cost-scope-member");
    let billing = fixture.member("billing");
    let absolute = billing.display().to_string();

    let reference = rhei_from(&fixture, &fixture.dir, &["cost", &absolute]);
    assert_success(&reference);
    assert!(
        reference.stdout.contains(&format!("Invocations {BILLING_RECORDS}")),
        "the member holds {BILLING_RECORDS} records of its own; got:\n{}",
        said(&reference)
    );

    // The directory, its index, `.` from inside it, `..` from its `tasks/`,
    // and no argument at all: the five ways an author reaches a workspace they
    // are standing in or beside.
    let tasks = billing.join("tasks");
    let spellings: [(&std::path::Path, Vec<&str>); 4] = [
        (&billing, vec!["cost", "."]),
        (&billing, vec!["cost", "index.rhei.md"]),
        (&tasks, vec!["cost", ".."]),
        (&billing, vec!["cost"]),
    ];
    for (cwd, argv) in spellings {
        let result = rhei_from(&fixture, cwd, &argv);
        assert_success(&result);
        assert_eq!(
            result.stdout,
            reference.stdout,
            "`rhei {}` from {} must read as the member's absolute path does; got:\n{}",
            argv.join(" "),
            cwd.display(),
            said(&result)
        );
    }

    let payload = cost_json(&fixture, &absolute, &[]);
    assert_eq!(
        payload_records(&payload),
        BILLING_RECORDS,
        "the member's records, not the project directory's; got: {}",
        digest(&payload)
    );
    assert_eq!(
        payload_tokens(&payload),
        BILLING_TOKENS,
        "the total identifies which root was read; got: {}",
        digest(&payload)
    );
}

/// Scenario 2 — The project spelling is the one a reader reaches for to ask what the
/// whole run cost, so it reports the union of every root — each record once.
/// The two single-file rheis root at the project directory itself, so a reading
/// that failed to deduplicate roots would count their records twice.
// §FS-rhei-panta.6.5
#[test]
fn the_project_spelling_reports_every_root_and_counts_each_record_once() {
    let fixture = scope_fixture("cost-scope-project");
    let payload = cost_json(&fixture, &fixture.project.display().to_string(), &[]);

    assert_eq!(
        payload_records(&payload),
        PROJECT_RECORDS,
        "the member, the shared project root and the basin, once each; got: {}",
        digest(&payload)
    );
    assert_eq!(
        payload_tokens(&payload),
        PROJECT_TOKENS,
        "a root read twice would total 107000 over 8 records; got: {}",
        digest(&payload)
    );

    // §FS-rhei-cost-accounting.8.4: `roots` rides on every reading, not only an
    // empty one, so a consumer can tell a total over one root from a total over
    // four without parsing text.
    let roots = payload["roots"].as_array().unwrap_or_else(|| {
        panic!("the payload should carry a `roots` array; got: {}", digest(&payload))
    });
    assert_eq!(
        roots.len() as u64,
        PROJECT_ROOTS,
        "one entry per root searched; got: {}",
        payload["roots"]
    );
    let counted: u64 =
        roots.iter().map(|root| root["invocation_count"].as_u64().unwrap_or_default()).sum();
    assert_eq!(
        counted, PROJECT_RECORDS,
        "the roots' counts should account for every record; got: {}",
        payload["roots"]
    );
}

/// Scenario 3 — `--rhei <id>` reaches one ticket's cost from the project spelling without
/// knowing where its directory sits, and reaches exactly what naming the
/// directory reaches.
// §FS-rhei-panta.6.5
#[test]
fn narrowing_to_a_member_equals_naming_that_member() {
    let fixture = scope_fixture("cost-scope-narrow");
    let project = fixture.project.display().to_string();
    let billing = fixture.member("billing").display().to_string();

    let narrowed = rhei_from(&fixture, &fixture.dir, &["cost", &project, "--rhei", "billing"]);
    assert_success(&narrowed);
    let named = rhei_from(&fixture, &fixture.dir, &["cost", &billing]);
    assert_success(&named);
    assert_eq!(
        narrowed.stdout,
        named.stdout,
        "`--rhei billing` and naming the member's directory are one reading; got:\n{}",
        said(&narrowed)
    );
}

/// Scenario 4 — The hard half of the rule: a single-file rhei's execution root is the
/// project directory, shared with the run root and with every other single-file
/// rhei. Narrowing to one of them must read that root and filter its records by
/// the rhei that owns the ticket, rather than reporting everything in it.
// §FS-rhei-panta.6.5
#[test]
fn narrowing_to_a_single_file_rhei_filters_the_root_it_shares() {
    let fixture = scope_fixture("cost-scope-shared-root");
    let project = fixture.project.display().to_string();

    let payload = cost_json(&fixture, &project, &["--rhei", "ledger"]);
    assert_eq!(
        payload_records(&payload),
        1,
        "`ledger` owns one of the two records in the shared root; got: {}",
        digest(&payload)
    );
    assert_eq!(
        payload_tokens(&payload),
        LEDGER_RECORD_TOKENS,
        "30000 would mean `spool`'s record came along with it; got: {}",
        digest(&payload)
    );
}

/// Scenario 5 — An id the project does not hold is an error naming the ids it does, the
/// way every other `--rhei` already answers one. A silently empty scope would
/// report a typo as a run that cost nothing.
// §FS-rhei-panta.6.5
#[test]
fn an_unknown_rhei_id_is_refused_and_names_the_available_ones() {
    let fixture = scope_fixture("cost-scope-unknown-rhei");
    let project = fixture.project.display().to_string();

    let result = rhei_from(&fixture, &fixture.dir, &["cost", &project, "--rhei", "nope"]);
    assert!(!result.status.success(), "an unknown rhei id must fail; got:\n{}", said(&result));
    let message = said(&result);
    assert!(
        message.contains("unknown rhei 'nope'"),
        "the error should name the id that was not found; got:\n{message}"
    );
    for id in ["billing", "ledger", "spool", "quiet", "basin"] {
        assert!(
            message.contains(id),
            "the error should list the available rhei ids, including {id}; got:\n{message}"
        );
    }
}

/// Scenario 6 — The ticket's second ask. Over a project holding no records at all the
/// answer is empty either way, and the only thing that distinguishes "this run
/// cost nothing" from "I looked in the wrong place" is naming where it looked.
// §FS-rhei-cost-accounting.8
#[test]
fn an_empty_project_names_every_root_it_searched() {
    let fixture = empty_scope_fixture("cost-scope-empty");
    let project = fixture.project.display().to_string();

    let result = rhei_from(&fixture, &fixture.dir, &["cost", &project]);
    assert_success(&result);
    let first = result.stdout.lines().next().unwrap_or_default();
    assert_eq!(
        first,
        "(no accounting records found)",
        "the published first line does not move; got:\n{}",
        said(&result)
    );
    assert!(
        result.stdout.contains(&format!("{PROJECT_ROOTS} accounting roots")),
        "the project has {PROJECT_ROOTS} roots once the shared one is counted once; got:\n{}",
        said(&result)
    );
    for root in ["billing", "quiet", "basin"] {
        let expected = fixture.member(root).join("runtime/accounting");
        assert!(
            result.stdout.contains(&expected.display().to_string()),
            "the tail should name {}; got:\n{}",
            expected.display(),
            said(&result)
        );
    }
    let shared = fixture.project.join("runtime/accounting");
    assert!(
        result.stdout.contains(&shared.display().to_string()),
        "the tail should name the project's own root, which the single-file rheis share; got:\n{}",
        said(&result)
    );
}

/// Scenario 7 — `rhei summary` resolves its positional exactly as `rhei cost` does
/// (§FS-rhei-summary.1), so it narrows with it — and one sentence must not
/// describe two scopes: the member's invocations beside the project's tally.
// §FS-rhei-summary.2.1
#[test]
fn summary_over_a_member_reports_that_members_invocations_and_tally() {
    let fixture = scope_fixture("summary-scope-member");
    let billing = fixture.member("billing").display().to_string();

    let result = rhei_from(&fixture, &fixture.dir, &["summary", &billing]);
    assert_success(&result);
    let lead = result.stdout.lines().next().unwrap_or_default();
    assert_eq!(
        lead,
        "`integration-test` workflow: 3 agent invocations across 1 model; \
         1 task completed, 1 in progress.",
        "the member holds three records and two tasks; got:\n{}",
        said(&result)
    );
    // §FS-rhei-summary.4: the output is publishable verbatim, so the roots line
    // `cost` prints is never part of it.
    assert!(
        !result.stdout.contains("accounting root"),
        "a summary never names a local directory; got:\n{}",
        said(&result)
    );
}

/// Scenario 8 — The regression guard, and the one scenario here that passes before the
/// change as well as after. A standalone workspace has one accounting root,
/// which is both the run root and its rhei's, so the union is the single read
/// it always was and its output does not move.
// §FS-rhei-cost-accounting.8.4
#[test]
fn a_standalone_workspace_reads_exactly_as_it_did() {
    let fixture = standalone_fixture("cost-scope-standalone");
    let solo = fixture.project.display().to_string();

    let result = rhei_from(&fixture, &fixture.dir, &["cost", &solo]);
    assert_success(&result);
    let first = result.stdout.lines().next().unwrap_or_default();
    assert_eq!(
        first,
        "Cost $0.00 | Total 10.0k | In 8.0k | Out 2.0k | Coverage Complete | Invocations 1",
        "the standalone reading is byte for byte what it was; got:\n{}",
        said(&result)
    );
    assert!(
        !result.stdout.contains("searched"),
        "a reading that found records prints no roots tail; got:\n{}",
        said(&result)
    );
}
