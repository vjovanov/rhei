"""A mock agent implementing the minimum native session contract.

It parses the session flags rhei passes (`--session-dir`, `--resume`, `--fork`,
`--interactive`), appends one line per invocation to a log, and writes the
session transcript rhei then snapshots. Python rather than a shell script, so
the example runs wherever `python3` is on `PATH`.
"""

# §REQ-cross-platform.4

import json
import os
import pathlib
import re
import sys


def env(name, default=""):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    return os.environ.get(name) or default


session_dir = ""
resume_value = ""
fork_value = ""
interactive = 0
prompt = ""

args = sys.argv[1:]
while args:
    flag = args.pop(0)
    if flag == "--interactive":
        interactive = 1
    elif flag == "--session-dir" and args:
        session_dir = args.pop(0)
    elif flag == "--resume" and args:
        resume_value = args.pop(0)
    elif flag == "--fork" and args:
        fork_value = args.pop(0)
    elif flag == "--prompt" and args:
        prompt = args.pop(0)
    elif flag == "--model" and args:
        args.pop(0)

# Autonomous wrappers read task identity from the authoritative prompt.
# §FS-rhei-agents.4
task_match = re.search(r"^# Task ([^:]+):", prompt, re.MULTILINE)
if not task_match:
    print("the agent prompt is missing its qualified task id", file=sys.stderr)
    sys.exit(1)
task_id = task_match.group(1)

log = pathlib.Path("runtime") / "fake-analysis-agent.log"
log.parent.mkdir(parents=True, exist_ok=True)
with log.open("a", encoding="utf-8", newline="") as handle:
    handle.write(
        "task={} state={} target={} resume={} fork={} interactive={} parent={}\n".format(
            task_id,
            env("RHEI_STATE"),
            env("RHEI_TARGET_SLUG"),
            resume_value,
            fork_value,
            interactive,
            env("RHEI_SNAPSHOT_PARENT_REF"),
        )
    )

if session_dir:
    directory = pathlib.Path(session_dir)
    directory.mkdir(parents=True, exist_ok=True)
    session_id = "{}-{}-{}".format(
        task_id, env("RHEI_STATE"), env("RHEI_TARGET_SLUG", "target")
    )
    lines = [
        {
            "session": {
                "provider": env("RHEI_MODEL_PROVIDER", "acme"),
                "model": env("RHEI_MODEL_NAME", "model-a"),
            }
        },
        {"role": "assistant", "content": env("RHEI_STATE"), "interactive": interactive},
    ]
    with (directory / (session_id + ".jsonl")).open("w", encoding="utf-8", newline="") as handle:
        for line in lines:
            handle.write(json.dumps(line) + "\n")
