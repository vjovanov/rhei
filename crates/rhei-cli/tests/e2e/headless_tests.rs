//! Detached runs end to end: `--headless`, `attach`, `runs`, `stop`, and the
//! `--json` record stream.
//!
//! These drive the real binary against a real detached process, because the
//! things worth proving here — that a run outlives its launcher, that a stop is
//! the interruption contract, that a second process can follow the first — are
//! not observable in-process.
// §FS-rhei-run-headless §FS-rhei-run-json

use std::fs;
use std::time::Duration;

use super::headless_support::{kinds, parse_records, stderr, stdout, wait_until, Workspace};

// ---------------------------------------------------------------------------
// §FS-rhei-run-json: the record stream
// ---------------------------------------------------------------------------

#[test]
fn json_run_puts_records_and_nothing_else_on_stdout() {
    let ws = Workspace::new("headless-json", 0);
    let out = ws.run(&["--json"]);
    assert!(out.status.success(), "run failed: {}", stderr(&out));

    let records = parse_records(&stdout(&out));
    let kinds = kinds(&records);
    assert_eq!(kinds.first().map(String::as_str), Some("run_started"));
    assert_eq!(kinds.last().map(String::as_str), Some("run_finished"));
    assert_eq!(kinds.iter().filter(|k| *k == "slot_assigned").count(), 2);
    assert_eq!(kinds.iter().filter(|k| *k == "slot_released").count(), 2);

    // The envelope: gap-free sequence from 1, and a timestamp on every record.
    for (index, record) in records.iter().enumerate() {
        assert_eq!(record["seq"], index as u64 + 1, "seq must be gap-free");
        assert!(record["ts"].as_str().is_some_and(|ts| ts.ends_with('Z')), "record {index} ts");
    }
    assert_eq!(records[0]["schema"], 1, "the head of the stream pins the schema");
    assert_eq!(records[0]["run_id"].as_str().map(str::len), Some(6));

    // Human prose went to stderr rather than being dropped. §FS-rhei-run-json.1
    assert!(stderr(&out).contains("Report:"), "stderr keeps the report pointer");
}

#[test]
fn the_durable_event_log_holds_the_same_records_the_stream_did() {
    let ws = Workspace::new("headless-log", 0);
    let out = ws.run(&["--json"]);
    assert!(out.status.success(), "run failed: {}", stderr(&out));

    let logged = fs::read_to_string(ws.root.join("runtime/events.jsonl")).expect("event log");
    assert_eq!(
        kinds(&parse_records(&logged)),
        kinds(&parse_records(&stdout(&out))),
        "the log and the stream are one contract, not two"
    );
}

#[test]
fn a_plain_run_still_writes_the_event_log_and_the_descriptor() {
    // Identity belongs to the run, not to a flag: a bare `rhei run` is
    // followable and has an id. §FS-rhei-run.2.7
    let ws = Workspace::new("headless-plain", 0);
    let out = ws.run(&["--no-tui"]);
    assert!(out.status.success(), "run failed: {}", stderr(&out));

    assert!(ws.root.join("runtime/events.jsonl").is_file(), "every run writes its event log");
    let descriptor = ws.descriptor().expect("every run publishes a descriptor");
    assert_eq!(descriptor["status"], "finished");
    assert_eq!(descriptor["exit_code"], 0);
    assert_eq!(descriptor["headless"], false);
    assert!(descriptor["log"].is_null(), "a foreground run has no console of its own");
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.1: launching
// ---------------------------------------------------------------------------

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn headless_launch_detaches_and_the_run_completes_on_its_own() {
    let ws = Workspace::new("headless-launch", 1);
    let id = ws.launch_headless();

    let descriptor = ws.descriptor().expect("descriptor published before the launcher returned");
    assert_eq!(descriptor["id"], id.as_str());
    assert_eq!(descriptor["status"], "running");
    assert_eq!(descriptor["headless"], true);
    assert!(
        descriptor["control_url"].as_str().is_some_and(|url| url.starts_with("http://127.0.0.1:")),
        "a detached run always serves its control endpoint (§FS-rhei-run-headless.4)"
    );

    wait_until("the detached run to finish", Duration::from_secs(60), || {
        ws.descriptor().is_some_and(|d| d["status"] == "finished")
    });

    let descriptor = ws.descriptor().expect("descriptor");
    assert_eq!(descriptor["exit_code"], 0);
    // The registry lists live runs, so a finished one has left it.
    let listed = ws.rhei(&["runs"]);
    assert!(stdout(&listed).contains("No runs are live"), "got: {}", stdout(&listed));
    assert!(
        ws.plan_text().matches("**State:** done").count() == 2,
        "the detached run really did the work:\n{}",
        ws.plan_text()
    );
}

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn headless_json_prints_one_descriptor_object() {
    let ws = Workspace::new("headless-launch-json", 1);
    let out = ws.run(&["--headless", "--json"]);
    assert!(out.status.success(), "launch failed: {}", stderr(&out));

    let records = parse_records(&stdout(&out));
    assert_eq!(records.len(), 1, "the launcher describes the run it started, once");
    assert_eq!(records[0]["headless"], true);
    assert!(records[0]["id"].as_str().is_some_and(|id| id.len() == 6));
    assert!(records[0]["pid"].as_u64().is_some());
}

/// Turning off a view the operator does not want must not also turn off the
/// ability to intervene in the run. §FS-rhei-run-headless.4
// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn no_dashboard_withholds_the_link_but_keeps_the_control_server() {
    let ws = Workspace::new("headless-nodash", 20);
    let out = ws.run(&["--headless", "--no-dashboard"]);
    assert!(out.status.success(), "launch failed: {}", stderr(&out));
    assert!(
        !stdout(&out).contains("browser:"),
        "nobody asked to be sent to a browser:\n{}",
        stdout(&out)
    );

    let descriptor = ws.descriptor().expect("descriptor");
    assert!(
        descriptor["control_url"].as_str().is_some_and(|url| url.starts_with("http://127.0.0.1:")),
        "an attached surface still needs somewhere to intervene through"
    );
}

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn a_second_headless_run_fails_synchronously_and_names_the_live_one() {
    // The lock refusal must arrive as a launcher failure, not as an id for a
    // run that dies a moment later. §FS-rhei-run-headless.1.1
    let ws = Workspace::new("headless-lock", 20);
    let id = ws.launch_headless();

    let second = ws.run(&["--headless"]);
    assert!(!second.status.success(), "a second run must not start");
    let message = format!("{}{}", stdout(&second), stderr(&second));
    assert!(message.contains("already live"), "got: {message}");
    assert!(message.contains(&id), "the diagnostic names the run in the way: {message}");
}

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn a_launch_that_cannot_start_reports_the_runs_own_diagnostic() {
    let ws = Workspace::new("headless-invalid", 0);
    fs::write(ws.plan(), "# Rhei: Broken\n\n## Tasks\n\n### Task 1: X\n**State:** nonexistent\n")
        .expect("write a plan the machine rejects");

    let out = ws.run(&["--headless"]);
    assert!(!out.status.success(), "an invalid plan must fail the launcher");
    let message = format!("{}{}", stdout(&out), stderr(&out));
    assert!(
        message.contains("exited before it started"),
        "the launcher explains that the run died: {message}"
    );
    assert!(
        message.contains("nonexistent"),
        "and shows the run's own console diagnostic: {message}"
    );
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.5: attaching
// ---------------------------------------------------------------------------

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn attach_json_follows_a_run_it_did_not_start_and_exits_with_its_code() {
    let ws = Workspace::new("headless-attach", 1);
    ws.launch_headless();

    let out = ws.rhei(&["attach", "--json", "--wait"]);
    assert!(out.status.success(), "the run succeeded, so --wait exits 0: {}", stderr(&out));

    let kinds = kinds(&parse_records(&stdout(&out)));
    assert_eq!(kinds.first().map(String::as_str), Some("run_started"));
    assert_eq!(
        kinds.iter().filter(|k| *k == "slot_released").count(),
        2,
        "a separate process saw every ticket's worker finish"
    );
    // `run_finished` terminates the run loop; only closing diagnostics may
    // follow it, and a detached run has one — the frozen dashboard's path.
    // §FS-rhei-run-json.2.4
    let finish = kinds.iter().position(|k| k == "run_finished").expect("the run ended");
    assert!(
        kinds[finish + 1..].iter().all(|k| k == "message" || k == "link"),
        "only closing notes follow run_finished, got: {:?}",
        &kinds[finish..]
    );
}

#[test]
fn attach_since_resumes_after_a_sequence_number() {
    let ws = Workspace::new("headless-since", 0);
    assert!(ws.run(&["--no-tui"]).status.success());

    let all = parse_records(&stdout(&ws.rhei(&["attach", "--json"])));
    assert!(all.len() > 3, "the finished run replays its whole log");

    let resumed = parse_records(&stdout(&ws.rhei(&["attach", "--json", "--since", "3"])));
    assert_eq!(resumed.len(), all.len() - 3, "--since skips exactly what it names");
    assert_eq!(resumed[0]["seq"], 4);
}

#[test]
fn attach_to_a_finished_run_reports_the_result_instead_of_opening_a_surface() {
    let ws = Workspace::new("headless-finished", 0);
    assert!(ws.run(&["--no-tui"]).status.success());

    let out = ws.rhei(&["attach"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("has ended"), "got: {text}");
    assert!(text.contains("exited 0"), "got: {text}");
    // What outlived the run is what the operator came back for.
    assert!(text.contains("run-report.md"), "got: {text}");
}

#[test]
fn an_unknown_run_reference_points_at_the_run_list() {
    let ws = Workspace::new("headless-unknown", 0);
    let out = ws.rhei(&["attach", "nosuch"]);
    assert!(!out.status.success());
    let message = format!("{}{}", stdout(&out), stderr(&out));
    assert!(message.contains("no run matches 'nosuch'"), "got: {message}");
    assert!(message.contains("rhei runs"), "got: {message}");
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.6: listing
// ---------------------------------------------------------------------------

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn runs_lists_the_live_run_and_json_describes_it() {
    let ws = Workspace::new("headless-runs", 20);
    let id = ws.launch_headless();

    let text = stdout(&ws.rhei(&["runs"]));
    assert!(text.contains(&id), "got: {text}");
    assert!(text.contains("headless"), "got: {text}");

    let listed: serde_json::Value =
        serde_json::from_str(&stdout(&ws.rhei(&["runs", "--json"]))).expect("json run list");
    let entries = listed.as_array().expect("an array of descriptors");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], id.as_str());
    assert_eq!(entries[0]["status"], "running");
}

#[test]
fn an_empty_run_list_is_an_answer_not_a_failure() {
    let ws = Workspace::new("headless-empty", 0);
    let out = ws.rhei(&["runs"]);
    assert!(out.status.success(), "nothing running is the normal state of a machine");
    assert!(stdout(&out).contains("No runs are live"));

    let json = ws.rhei(&["runs", "--json"]);
    assert!(json.status.success());
    assert_eq!(stdout(&json).trim(), "[]");
}

// ---------------------------------------------------------------------------
// §FS-rhei-run-headless.7: stopping
// ---------------------------------------------------------------------------

// Unix-only: `--headless` needs a POSIX session to detach into, and says so
// on every other platform. §FS-rhei-run-headless.1.3
#[test]
fn stop_interrupts_the_run_and_leaves_its_in_flight_ticket_alone() {
    let ws = Workspace::new("headless-stop", 30);
    let id = ws.launch_headless();
    wait_until("the run to claim a ticket", Duration::from_secs(30), || {
        fs::read_to_string(ws.root.join("runtime/events.jsonl"))
            .is_ok_and(|log| log.contains("slot_assigned"))
    });

    let out = ws.rhei(&["stop", &id, "--wait"]);
    assert!(out.status.success(), "stopping is not a failure: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Asked run"), "got: {text}");
    assert!(text.contains("has ended"), "got: {text}");
    // The interruption contract, unchanged: 128 + SIGINT. §FS-rhei-run.3.2
    assert!(text.contains("exited 130"), "got: {text}");

    // No transition fired for the interrupted invocation: the ticket kept the
    // state it was in and the next run re-executes it. §FS-rhei-run.3.2
    let plan = ws.plan_text();
    assert_eq!(
        plan.matches("**State:** pending").count(),
        2,
        "an interruption is neither a failure nor a completion:\n{plan}"
    );
}

#[test]
fn stopping_a_run_that_already_ended_is_not_an_error() {
    let ws = Workspace::new("headless-stop-done", 0);
    assert!(ws.run(&["--no-tui"]).status.success());

    let out = ws.rhei(&["stop"]);
    assert!(out.status.success(), "the intent — not running — is already satisfied");
    assert!(stdout(&out).contains("already ended"), "got: {}", stdout(&out));
}

// ---------------------------------------------------------------------------
// Detachment itself
// ---------------------------------------------------------------------------

/// The point of the feature: the run is its own session leader, so the
/// terminal that launched it can go away without taking it down.
// §FS-rhei-run-headless.1 §FS-rhei-run-headless.8
// Unix-only: a session leader is a POSIX session, and `setsid` is what makes
// one.
#[cfg(unix)]
#[test]
fn a_detached_run_is_its_own_session_leader() {
    let ws = Workspace::new("headless-session", 20);
    ws.launch_headless();
    let pid = ws.descriptor().expect("descriptor")["pid"].as_u64().expect("pid") as i32;

    let session = fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|stat| {
        // `sid` is field 6, after the comm field, which may itself contain
        // spaces — so split on the closing paren rather than on whitespace.
        let tail = stat.rsplit_once(") ")?.1;
        tail.split_whitespace().nth(3)?.parse::<i32>().ok()
    });
    // `session` is `None` off Linux: procfs is where the session id is readable,
    // so on another Unix there is nothing to assert here — an unavailable
    // assertion, not a failed one.
    if let Some(sid) = session {
        assert_eq!(
            sid, pid,
            "a detached run leads its own session, so the launcher's SIGHUP cannot reach it"
        );
    }
}
