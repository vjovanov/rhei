"""gh-ci-status.py — tri-state GitHub CI status probe for rhei `ci-watch`.

Contract (see ../index.rhei.md §Status-check contract):
  exit 0  — every required check passed
  exit 1  — at least one required check failed
  exit 75 — checks are still running; retry after poll.interval

Inputs (environment): BRANCH, the branch under observation, and REPORT_PATH,
where to write the JSON report (from `{output.ci-report.path}` in states.yaml).

Requires an authenticated `gh`. Python rather than a shell script, so the
example runs wherever `python3` is on `PATH`; `jq` is no longer needed either,
the JSON `gh` returns being read here directly. §REQ-cross-platform.4
"""

import json
import os
import pathlib
import subprocess
import sys


def required(name):
    value = os.environ.get(name)
    if not value:
        sys.exit(f"{name} is required")
    return value


BRANCH = required("BRANCH")
REPORT_PATH = pathlib.Path(required("REPORT_PATH"))
REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)


def gh(*args):
    out = subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def write_report(payload):
    REPORT_PATH.write_text(json.dumps(payload), encoding="utf-8")


def record_result(verdict):
    # `ci-watch` can route straight into `heal-done` (green) or `poll-gave-up`
    # (budget spent), and a `final: true` state is not entered without a result.
    # The program is the worker here, so it records the probe's verdict on every
    # attempt; whichever attempt ends the ticket, the reason is already written.
    # §FS-rhei-states.3.3 §FS-rhei-programs.2
    path = os.environ.get("RHEI_RESULT_PATH")
    if path:
        path = pathlib.Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"## Result\n\n{verdict} Report: `{REPORT_PATH}`.\n", encoding="utf-8")


# Latest run on this branch. `gh run list` returns most recent first.
runs = gh("run", "list", "--branch", BRANCH, "--limit", "1",
          "--json", "databaseId,headSha,status,conclusion")
if not runs:
    write_report({"branch": BRANCH, "sha": None, "jobs": [], "note": "no runs found"})
    # No run yet — treat as still-pending so the poll loop keeps waiting.
    record_result(f"No CI run found for `{BRANCH}` within the poll budget.")
    sys.exit(75)

run = runs[0]
sha = run["headSha"]
jobs = gh("run", "view", str(run["databaseId"]), "--json", "jobs")["jobs"]
write_report({"branch": BRANCH, "sha": sha, "jobs": [
    {"name": job["name"], "status": job.get("conclusion") or job["status"], "log_url": job["url"]}
    for job in jobs
]})

if run["status"] != "completed":  # queued | in_progress | waiting | requested
    record_result(f"CI on `{BRANCH}` at `{sha}` never returned a verdict "
                  f"(last status: {run['status']}).")
    sys.exit(75)

if run["conclusion"] == "success":
    record_result(f"CI is green on `{BRANCH}` at `{sha}`.")
    sys.exit(0)

# failure, cancelled, timed_out, action_required, ...
record_result(f"CI failed on `{BRANCH}` at `{sha}` ({run['conclusion']}).")
sys.exit(1)
