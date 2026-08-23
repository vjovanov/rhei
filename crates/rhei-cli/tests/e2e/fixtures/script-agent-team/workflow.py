"""The team's mock workflow, driven from the state machine's `cli:` callbacks.

Python rather than a shell script so the fixture runs on every platform the
CLI is tested on: a `cli:` callback goes to the platform's own shell, and `sh`
and `cmd` share almost no vocabulary. Nothing here needs a shell.
"""

import datetime
import os
import pathlib
import sys


def env(name, default=""):
    value = os.environ.get(name)
    return value if value else default


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def write_file(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    # `newline=""` so a line ends with one `\n` on every platform: Python's text
    # mode would write `\r\n` on Windows and the artifact would differ by host.
    with path.open("w", encoding="utf-8", newline="") as handle:
        handle.write(content + "\n")


def main():
    command_name = sys.argv[1] if len(sys.argv) > 1 else ""

    plan_path = env("RHEI_PLAN_PATH")
    if not plan_path:
        print("RHEI_PLAN_PATH is required", file=sys.stderr)
        return 1
    plan_path = pathlib.Path(plan_path)
    workspace_root = plan_path if plan_path.is_dir() else plan_path.parent

    task_id = env("RHEI_TASK_ID", "unknown")
    runtime_dir = workspace_root / "runtime"
    logs_dir = runtime_dir / "logs"
    artifacts_dir = runtime_dir / "artifacts" / ("task-" + task_id)
    logs_dir.mkdir(parents=True, exist_ok=True)
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    team_log = logs_dir / "team.log"
    task_log = logs_dir / ("task-" + task_id + ".log")

    def log_line(message):
        line = "{} task={} {} -> {} {}\n".format(
            timestamp(),
            task_id,
            env("RHEI_FROM_STATE", "unknown"),
            env("RHEI_TO_STATE", "unknown"),
            message,
        )
        for log in (team_log, task_log):
            with log.open("a", encoding="utf-8", newline="") as handle:
                handle.write(line)

    if command_name == "kickoff-mock":
        log_line("mock kickoff command executed")
        write_file(
            artifacts_dir / "00-kickoff.txt",
            "mock-command: agent-team kickoff --task " + task_id,
        )
    elif command_name == "handoff-research":
        log_line("coordinator handed task to researcher")
        write_file(
            artifacts_dir / "10-research-note.md",
            "# Research Note for Task {}\n"
            "\n"
            "- Source state: {}\n"
            "- Target state: {}\n"
            "- Summary: gather context, keep it small, hand off clearly.".format(
                task_id, env("RHEI_FROM_STATE"), env("RHEI_TO_STATE")
            ),
        )
    elif command_name == "handoff-implementation":
        log_line("researcher handed task to implementer")
        write_file(
            artifacts_dir / "20-implementation.txt",
            "implementation artifact for task {}\n"
            "based on: 10-research-note.md".format(task_id),
        )
    elif command_name == "handoff-review":
        log_line("implementer handed task to reviewer")
        if not (artifacts_dir / "20-implementation.txt").is_file():
            print(
                "missing implementation artifact for task " + task_id, file=sys.stderr
            )
            return 1
        write_file(
            artifacts_dir / "30-review.txt",
            "review prepared for task {}\nartifact present: yes".format(task_id),
        )
    elif command_name == "finalize":
        log_line("reviewer finalized task")
        write_file(
            artifacts_dir / "40-complete.txt",
            "task {} completed at {}".format(task_id, timestamp()),
        )
    elif command_name == "cancel":
        log_line("task cancelled")
        write_file(
            artifacts_dir / "99-cancelled.txt",
            "task {} cancelled at {}".format(task_id, timestamp()),
        )
    else:
        print("unknown workflow command: " + command_name, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
