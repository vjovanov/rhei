// §AR-source-file-size.3

// `rhei runs` as a question about history rather than only about right now, and
// the retention boundary that makes some windows unanswerable.

use std::fs;
use std::path::{Path, PathBuf};

use super::{rhei_command, unique_temp_dir, CliRun, TestDir};

/// The registry's cap on retained ended entries. §FS-rhei-run-headless.2
const RETAINED_ENDED_RUNS: usize = 100;

/// A home with its own registry, and `n` ended runs already in it.
///
/// Each entry needs a workspace descriptor that still names it, because that is
/// what tells a sweep the entry is over rather than prunable.
// §FS-rhei-run-headless.2 §FS-rhei-run-headless.3
fn home_with_ended_runs(prefix: &str, n: usize) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let home = dir.join("home");
    let registry = home.join("state/rhei/runs");
    fs::create_dir_all(&registry).expect("create registry dir");
    for index in 0..n {
        let id = format!("hist{index:03}");
        let workspace = dir.join(format!("ws{index:03}"));
        fs::create_dir_all(workspace.join("runtime")).expect("create workspace runtime");
        let descriptor = serde_json::json!({
            "id": id,
            "pid": 1_u32,
            "status": "finished",
            "workspace": workspace,
            "plan": workspace.join("plan.rhei.md"),
            "started_at": format!("2026-01-01T{:02}:{:02}:00Z", index / 60, index % 60),
            "headless": false,
            "parallel": 1,
            "events": workspace.join("runtime/events.jsonl"),
            "exit_code": 0,
        });
        let body = serde_json::to_string_pretty(&descriptor).expect("descriptor serializes");
        fs::write(workspace.join("runtime/run.json"), &body).expect("write workspace descriptor");
        fs::write(registry.join(format!("{id}.json")), &body).expect("write registry entry");
    }
    (dir, home)
}

fn runs(home: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(home);
    cmd.arg("runs");
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei runs should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A run list that only ever shows what is live cannot name the runs a window
/// contains, which is what a cost report has to join against.
// §FS-rhei-run-headless.6.1
#[test]
fn runs_all_lists_the_ended_runs_the_registry_still_holds() {
    let (_dir, home) = home_with_ended_runs("runs-all", 3);

    let result = runs(&home, &["--all"]);
    assert!(
        result.status.success(),
        "rhei runs --all should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    for id in ["hist000", "hist001", "hist002"] {
        assert!(result.stdout.contains(id), "{id} should be listed; got:\n{}", result.stdout);
    }

    let windowed = runs(&home, &["--since", "2026-01-01", "--until", "2026-01-02"]);
    assert!(
        windowed.status.success(),
        "a window should succeed\nstdout:\n{}\nstderr:\n{}",
        windowed.stdout,
        windowed.stderr
    );
    assert!(
        windowed.stdout.contains("hist002"),
        "a window implies history; got:\n{}",
        windowed.stdout
    );
}

/// The cap is right for resolving an id and wrong for a window: past it the
/// entries were unlinked and there is nothing left to count. Saying so is the
/// difference between an incomplete answer and a wrong one.
// §FS-rhei-run-headless.6.2
#[test]
fn runs_says_what_retention_hid_rather_than_summing_what_is_left() {
    let (_dir, home) = home_with_ended_runs("runs-truncated", RETAINED_ENDED_RUNS + 4);

    let result = runs(&home, &["--all"]);
    assert!(
        result.status.success(),
        "rhei runs --all should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Retention truncated this window:"),
        "the listing must say what it could not see; got:\n{}",
        result.stdout
    );

    let payload: serde_json::Value = {
        let json = runs(&home, &["--all", "--json"]);
        assert!(
            json.status.success(),
            "rhei runs --all --json should succeed\nstdout:\n{}\nstderr:\n{}",
            json.stdout,
            json.stderr
        );
        serde_json::from_str(&json.stdout).unwrap_or_else(|err| {
            panic!("history JSON should parse ({err}); got:\n{}", json.stdout)
        })
    };
    assert_eq!(payload["schema"].as_str(), Some("rhei.run-history.v1"));
    assert!(
        !payload["truncated"].is_null(),
        "the machine-readable answer carries the same fact; got:\n{payload:#?}"
    );
}

/// At exactly the cap, with nothing ever unlinked, the notice still fires — the
/// registry cannot know whether a sweep ran before it — but it may not assert
/// that runs were dropped. Claiming a deletion that never happened is the same
/// kind of wrong answer as summing a shortened window.
// §FS-rhei-run-headless.6.2
#[test]
fn the_truncation_notice_claims_no_deletion_the_registry_cannot_know_about() {
    let (_dir, home) = home_with_ended_runs("runs-at-cap", RETAINED_ENDED_RUNS);

    let result = runs(&home, &["--all"]);
    assert!(
        result.status.success(),
        "rhei runs --all should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Retention truncated this window:"),
        "a registry standing at its cap still bounds the window; got:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("were unlinked"),
        "nothing was unlinked here, so the listing may not say so; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("may reach past what the registry still holds"),
        "the honest claim is the one true whether or not the sweep ran; got:\n{}",
        result.stdout
    );
}

/// And with no flag at all, the command is what it has always been.
// §FS-rhei-run-headless.6
#[test]
fn runs_with_no_flag_still_lists_only_what_is_live() {
    let (_dir, home) = home_with_ended_runs("runs-compat", 3);

    let result = runs(&home, &[]);
    assert!(result.status.success(), "got:\n{}\n{}", result.stdout, result.stderr);
    assert!(
        result.stdout.starts_with("No runs are live on this machine."),
        "ended runs are not live runs; got:\n{}",
        result.stdout
    );
    assert!(!result.stdout.contains("hist000"), "got:\n{}", result.stdout);

    let json = runs(&home, &["--json"]);
    assert!(json.status.success(), "got:\n{}\n{}", json.stdout, json.stderr);
    assert_eq!(json.stdout.trim(), "[]", "`--json` alone still emits the bare array");
}
