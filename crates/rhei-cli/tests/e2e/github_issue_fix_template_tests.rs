use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

use super::*;

const PROPOSAL_ID: &str = "0123456789abcdef";
const ACTOR: &str = "rhei-bot";

const FAKE_GH: &str = r#"#!/usr/bin/env python3
import json, os, sys
from pathlib import Path

root = Path(os.environ["FAKE_GH_ROOT"])
state_path = root / "state.json"
log_path = root / "calls.jsonl"
state = json.loads(state_path.read_text())
args = sys.argv[1:]
body = sys.stdin.read()
with log_path.open("a") as log:
    log.write(json.dumps({"args": args, "body": body}) + "\n")

method = "GET"
if "--method" in args:
    method = args[args.index("--method") + 1]
endpoint = next((a for a in reversed(args) if a.startswith("repos/")), "")

def save():
    state_path.write_text(json.dumps(state))

if endpoint.endswith("/comments") and method == "GET":
    comments = state.get("comments", [])
    print(json.dumps([comments] if "--slurp" in args else comments))
elif endpoint.endswith("/comments") and method == "POST":
    payload = json.loads(body)
    comments = state.setdefault("comments", [])
    comment = {
        "id": max([c.get("id", 0) for c in comments] + [0]) + 1,
        "created_at": f"2026-01-01T00:00:{len(comments) + 1:02d}Z",
        "body": payload["body"],
        "user": {"login": os.environ.get("FAKE_GH_ACTOR", "rhei-bot")},
    }
    comments.append(comment)
    save()
    print(json.dumps(comment))
elif "/collaborators/" in endpoint and endpoint.endswith("/permission"):
    login = endpoint.split("/collaborators/", 1)[1].split("/", 1)[0]
    permission = state.get("permissions", {}).get(login)
    if permission is None:
        print("HTTP 404: Not Found", file=sys.stderr)
        sys.exit(1)
    print(json.dumps({"permission": permission}))
elif "/labels/rhei:awaiting-approval" in endpoint and method == "DELETE":
    state["issue_labels"] = [
        name for name in state.get("issue_labels", [])
        if name != "rhei:awaiting-approval"
    ]
    save()
    print("[]")
elif "/labels/rhei:awaiting-approval" in endpoint:
    if not state.get("label_exists", True):
        print("HTTP 404: Not Found", file=sys.stderr)
        sys.exit(1)
    print(json.dumps({"name": "rhei:awaiting-approval"}))
elif endpoint.endswith("/labels") and method == "POST":
    if state.get("fail_label_once", False):
        state["fail_label_once"] = False
        save()
        print("label write failed", file=sys.stderr)
        sys.exit(1)
    labels = state.setdefault("issue_labels", [])
    if "rhei:awaiting-approval" not in labels:
        labels.append("rhei:awaiting-approval")
    save()
    print(json.dumps([{"name": name} for name in labels]))
elif endpoint.count("/") == 4 and "/issues/" in endpoint:
    print(json.dumps({
        "labels": [{"name": name} for name in state.get("issue_labels", [])]
    }))
else:
    print(f"unsupported fake gh request: {method} {endpoint}", file=sys.stderr)
    sys.exit(2)
"#;

struct Fixture {
    root: PathBuf,
    fake_bin: PathBuf,
    helper: PathBuf,
}

impl Fixture {
    fn new(state: Value) -> Self {
        let root = unique_scratchpad_dir("github-proposal-helper");
        let fake_bin = root.join("bin");
        fs::create_dir_all(&fake_bin).expect("create fake bin");
        let gh = fake_bin.join("gh");
        fs::write(&gh, FAKE_GH).expect("write fake gh");
        let mut permissions = fs::metadata(&gh).expect("fake gh metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&gh, permissions).expect("make fake gh executable");
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap())
            .expect("write fake state");
        Self {
            root,
            fake_bin,
            helper: repo_root().join(".agents/rhei/templates/github-issue-fix/bin/github-proposal"),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.helper);
        let path = format!("{}:{}", self.fake_bin.display(), env::var("PATH").unwrap_or_default());
        command.env("PATH", path).env("FAKE_GH_ROOT", &self.root).env("FAKE_GH_ACTOR", ACTOR);
        command
    }

    fn state(&self) -> Value {
        serde_json::from_slice(&fs::read(self.root.join("state.json")).unwrap()).unwrap()
    }

    fn call_count(&self) -> usize {
        fs::read_to_string(self.root.join("calls.jsonl"))
            .map(|contents| contents.lines().count())
            .unwrap_or(0)
    }
}

fn comment(id: u64, seconds: u64, author: &str, body: &str) -> Value {
    json!({
        "id": id,
        "created_at": format!("2026-01-01T00:00:{seconds:02}Z"),
        "body": body,
        "user": {"login": author}
    })
}

fn inspect(fixture: &Fixture, max_attempts: u64) -> Output {
    fixture
        .command()
        .args([
            "inspect",
            "--repo",
            "owner/repo",
            "--issue",
            "7",
            "--actor",
            ACTOR,
            "--max-attempts",
            &max_attempts.to_string(),
        ])
        .output()
        .expect("run inspect")
}

fn output_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("helper stdout is JSON")
}

fn base_state(comments: Vec<Value>, permissions: HashMap<&str, &str>) -> Value {
    json!({
        "comments": comments,
        "permissions": permissions,
        "label_exists": true,
        "issue_labels": []
    })
}

#[test]
fn proposal_rendering_is_deterministic_provenance_bearing_and_no_pr_is_offline() {
    let fixture = Fixture::new(base_state(vec![], HashMap::new()));
    let proposal = fixture.root.join("proposal.md");
    fs::write(&proposal, "Change one thing.\n\nValidate it.  \n").unwrap();
    let invocations = fixture.root.join("invocations");
    fs::create_dir_all(&invocations).unwrap();
    fs::write(
        invocations.join("proposal.json"),
        serde_json::to_vec(&json!({
            "state": "propose-fix",
            "provider": "openai",
            "model": "gpt-test",
            "started_at": "2026-01-01T00:00:00Z",
            "ended_at": "2026-01-01T00:00:01Z"
        }))
        .unwrap(),
    )
    .unwrap();

    let run = |path: &Path| {
        fixture
            .command()
            .args([
                "publish",
                "--repo",
                "owner/repo",
                "--issue",
                "7",
                "--actor",
                ACTOR,
                "--proposal",
                proposal.to_str().unwrap(),
                "--attempt",
                "1",
                "--invocations-dir",
                invocations.to_str().unwrap(),
                "--publication-mode",
                "no-pr",
                "--rendered-output",
                path.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let first_path = fixture.root.join("first.md");
    let second_path = fixture.root.join("second.md");
    let first = run(&first_path);
    let second = run(&second_path);
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(fs::read(&first_path).unwrap(), fs::read(&second_path).unwrap());
    assert_eq!(fixture.call_count(), 0, "no-pr must not invoke gh");

    let rendered = fs::read_to_string(first_path).unwrap();
    let id = output_json(&first)["proposal_id"].as_str().unwrap().to_owned();
    assert!(rendered.contains(&format!("<!-- rhei-proposal:v1 id={id} attempt=1 -->")));
    assert!(rendered.contains(&format!("/rhei approve {id}")));
    assert!(rendered.contains(&format!("/rhei reject {id}\n<explain what should change>")));
    assert!(rendered.contains("generated by AI using `openai:gpt-test`"));
    assert!(rendered.contains("[Rhei](https://github.com/vjovanov/rhei)"));

    fs::write(&proposal, "Change a different thing.\n").unwrap();
    let third = run(&fixture.root.join("third.md"));
    assert_ne!(output_json(&first)["proposal_id"], output_json(&third)["proposal_id"]);
}

#[test]
fn inspection_enforces_exact_current_authorized_commands() {
    // The configured publishing actor may approve or reject when authorized.
    // §FS-rhei-templates.11.1.
    for (command, exit, decision) in [("approve", 12, "approved"), ("reject", 13, "rejected")] {
        let marker = format!("<!-- rhei-proposal:v1 id={PROPOSAL_ID} attempt=1 -->");
        let fixture = Fixture::new(base_state(
            vec![
                comment(1, 1, ACTOR, &marker),
                comment(2, 2, ACTOR, &format!("/rhei {command} {PROPOSAL_ID}")),
            ],
            HashMap::from([(ACTOR, "write")]),
        ));
        let result = inspect(&fixture, 3);
        assert_eq!(result.status.code(), Some(exit));
        assert_eq!(output_json(&result)["decision"], decision);
        assert_eq!(output_json(&result)["decision_author"], ACTOR);
        assert_eq!(output_json(&result)["decision_permission"], "write");
    }

    for permission in ["write", "maintain", "admin"] {
        let marker = format!("<!-- rhei-proposal:v1 id={PROPOSAL_ID} attempt=1 -->");
        let fixture = Fixture::new(base_state(
            vec![
                comment(1, 1, ACTOR, &marker),
                comment(2, 2, "maintainer", &format!("/rhei approve {PROPOSAL_ID}")),
            ],
            HashMap::from([("maintainer", permission)]),
        ));
        let result = inspect(&fixture, 3);
        assert_eq!(result.status.code(), Some(12));
        assert_eq!(output_json(&result)["decision"], "approved");
        assert_eq!(output_json(&result)["decision_permission"], permission);
    }

    let marker = format!("<!-- rhei-proposal:v1 id={PROPOSAL_ID} attempt=2 -->");
    let fixture = Fixture::new(base_state(
        vec![
            comment(1, 1, ACTOR, &marker),
            comment(2, 2, "writer", "/rhei approve fedcba9876543210"),
            comment(3, 3, "writer", &format!(" /rhei approve {PROPOSAL_ID}")),
            comment(4, 4, "reader", &format!("/rhei approve {PROPOSAL_ID}")),
            comment(5, 5, "triager", &format!("/rhei approve {PROPOSAL_ID}")),
            comment(6, 6, "outsider", &format!("/rhei approve {PROPOSAL_ID}")),
            comment(7, 7, ACTOR, &format!("/rhei approve {PROPOSAL_ID}")),
        ],
        HashMap::from([("writer", "write"), ("reader", "read"), ("triager", "triage")]),
    ));
    let pending = inspect(&fixture, 3);
    assert_eq!(pending.status.code(), Some(11));
    assert_eq!(output_json(&pending)["decision"], "pending");

    let fixture = Fixture::new(base_state(
        vec![
            comment(1, 1, ACTOR, &marker),
            comment(
                2,
                2,
                "writer",
                &format!("/rhei reject {PROPOSAL_ID}\nPlease cover the retry case."),
            ),
        ],
        HashMap::from([("writer", "write")]),
    ));
    let rejected = inspect(&fixture, 3);
    assert_eq!(rejected.status.code(), Some(13));
    assert_eq!(output_json(&rejected)["rejection_feedback"], "Please cover the retry case.");
    let exhausted = inspect(&fixture, 2);
    assert_eq!(exhausted.status.code(), Some(14));
    assert_eq!(output_json(&exhausted)["decision"], "attempts-exhausted");
}

#[test]
fn publication_and_label_changes_are_idempotent_across_partial_failures() {
    let mut state = base_state(vec![], HashMap::new());
    state["fail_label_once"] = json!(true);
    let fixture = Fixture::new(state);
    let proposal = fixture.root.join("proposal.md");
    fs::write(&proposal, "A proposal.\n").unwrap();
    let publish = || {
        fixture
            .command()
            .args([
                "publish",
                "--repo",
                "owner/repo",
                "--issue",
                "7",
                "--actor",
                ACTOR,
                "--proposal",
                proposal.to_str().unwrap(),
                "--attempt",
                "1",
                "--invocations-dir",
                fixture.root.to_str().unwrap(),
                "--publication-mode",
                "draft",
            ])
            .output()
            .unwrap()
    };
    assert_eq!(publish().status.code(), Some(20));
    assert_eq!(fixture.state()["comments"].as_array().unwrap().len(), 1);
    let retry = publish();
    assert!(retry.status.success());
    assert_eq!(fixture.state()["comments"].as_array().unwrap().len(), 1);
    assert_eq!(fixture.state()["issue_labels"], json!(["rhei:awaiting-approval"]));

    let remove = fixture
        .command()
        .args(["label", "--repo", "owner/repo", "--issue", "7", "--action", "remove"])
        .output()
        .unwrap();
    assert!(remove.status.success());
    assert_eq!(fixture.state()["issue_labels"], json!([]));

    let missing = Fixture::new(json!({
        "comments": [],
        "permissions": {},
        "label_exists": false,
        "issue_labels": []
    }));
    fs::write(missing.root.join("proposal.md"), "Missing label.\n").unwrap();
    let result = missing
        .command()
        .args([
            "publish",
            "--repo",
            "owner/repo",
            "--issue",
            "7",
            "--actor",
            ACTOR,
            "--proposal",
            missing.root.join("proposal.md").to_str().unwrap(),
            "--attempt",
            "1",
            "--invocations-dir",
            missing.root.to_str().unwrap(),
            "--publication-mode",
            "draft",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(20));
    assert!(output_json(&result)["error"]
        .as_str()
        .unwrap()
        .contains("required label does not exist"));
}

#[test]
fn rendered_modes_validate_the_complete_approval_state_graph() {
    let template = repo_root().join(".agents/rhei/templates/github-issue-fix");
    for mode in ["no-pr", "draft"] {
        let root = unique_scratchpad_dir(&format!("github-approval-{mode}"));
        let output = root.join("out");
        let result = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .args([
                "instantiate",
                template.to_str().unwrap(),
                "7",
                "--set",
                "repo=owner/repo",
                "--set",
                "repo_checkout=/tmp",
                "--set",
                &format!("publication_mode={mode}"),
                "--output",
                output.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "instantiate {mode} failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let states = fs::read_to_string(output.join("states.yaml")).unwrap();
        for required in [
            "from: approval-check",
            "to: approval-apply",
            "to: rejection-prepare",
            "to: proposal-pending",
            "from: implementation-dispatch",
            "exit_code: 3",
            "handoff-provenance",
            "[Rhei](https://github.com/vjovanov/rhei)",
        ] {
            assert!(states.contains(required), "{mode} is missing {required}");
        }
        // Initial proposal generation has no proposal artifact, while fresh
        // rejection recovery has only the published proposal. §FS-rhei-templates.11.2.
        let propose = states
            .split("  propose-fix:")
            .nth(1)
            .unwrap()
            .split("  publish-proposal:")
            .next()
            .unwrap();
        for optional_input in ["previous-local-proposal", "previous-published-proposal"] {
            let input = propose
                .split(&format!("- name: {optional_input}"))
                .nth(1)
                .unwrap_or_else(|| panic!("{mode} is missing {optional_input}"));
            assert!(
                input.lines().take(4).any(|line| line.trim() == "optional: true"),
                "{mode} must make {optional_input} optional"
            );
        }
    }
}
