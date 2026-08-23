"""commit-and-push.py — commit the agent's fix and push to the branch
under observation. Paired with the `push-fix` state in states.yaml.

Inputs (environment):
  BRANCH        — branch to push to (required)
  SUMMARY_PATH  — path to the fix-summary markdown the agent wrote
                  (required; used as the commit body)

Exit 0 on success; non-zero on any git failure. Python rather than a shell
script, so the example runs wherever `python3` is on `PATH`.
§REQ-cross-platform.4
"""

import os
import pathlib
import subprocess
import sys


def required(name):
    value = os.environ.get(name)
    if not value:
        sys.exit(f"{name} is required")
    return value


def git(*args, **kwargs):
    return subprocess.run(["git", *args], check=True, text=True, **kwargs)


BRANCH = required("BRANCH")
SUMMARY_PATH = pathlib.Path(required("SUMMARY_PATH"))

if not SUMMARY_PATH.is_file() or SUMMARY_PATH.stat().st_size == 0:
    sys.exit(f"commit-and-push: fix summary missing or empty: {SUMMARY_PATH}")

current_branch = git("rev-parse", "--abbrev-ref", "HEAD", capture_output=True).stdout.strip()
if current_branch != BRANCH:
    sys.exit(f"commit-and-push: expected branch '{BRANCH}', on '{current_branch}'")

unstaged = subprocess.run(["git", "diff", "--quiet"]).returncode
staged = subprocess.run(["git", "diff", "--cached", "--quiet"]).returncode
if unstaged == 0 and staged == 0:
    print("commit-and-push: nothing to commit; skipping push.", file=sys.stderr)
    sys.exit(0)

summary = SUMMARY_PATH.read_text(encoding="utf-8")
subject = summary.splitlines()[0].removeprefix("# ").strip() or "apply fix"
message = f"ci-heal: {subject}\n\n{summary}"

git("add", "-A")
git("commit", "--file=-", input=message)
git("push", "origin", BRANCH)
