"""The living review loop's workflow, driven from the machine's `cli:` callbacks.

Python rather than a shell script so the fixture runs on every platform the CLI
is tested on: a `cli:` callback goes to the platform's own shell, and `sh` and
`cmd` share almost no vocabulary. The live-reviewer path execs `claude` and
`codex` directly, never through a shell.
"""

import datetime
import os
import pathlib
import subprocess
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


def write_task_if_missing(path, content):
    if path.exists():
        return
    write_file(path, content)


def review_prompt(model):
    return (
        "Review the Rhei specification documents under docs/functional-spec/ for"
        " gaps, contradictions, and ambiguities.\n"
        "Focus on problems that would mislead an implementor or confuse a user.\n"
        "\n"
        "Respond with markdown only in this exact shape:\n"
        "# Review Findings: Model {}\n"
        "\n"
        "- F-...: ...\n"
        "- F-...: ...\n"
        "\n"
        "Keep the findings concise and distinct from the other reviewer.\n".format(model)
    )


MOCK_REVIEWS = {
    "claude": "# Review Findings: Model claude\n"
    "\n"
    "- F-001: cache invalidation key appears to omit the project identifier\n"
    "- F-002: release example help text may still mention a stale flag",
    "codex": "# Review Findings: Model codex\n"
    "\n"
    "- F-001: cache key composition looks incomplete around project scoping\n"
    "- F-003: retry path may swallow the upstream timeout detail",
}


class Workflow:
    def __init__(self, workspace_root):
        self.workspace_root = workspace_root
        self.task_id = env("RHEI_TASK_ID", "unknown")
        self.runtime_dir = workspace_root / "runtime"
        self.logs_dir = self.runtime_dir / "logs"
        self.findings_dir = self.runtime_dir / "findings"
        self.verifications_dir = self.runtime_dir / "verifications"
        self.fixes_dir = self.runtime_dir / "fixes"
        self.tasks_dir = workspace_root / "tasks"
        for directory in (
            self.logs_dir,
            self.findings_dir,
            self.verifications_dir,
            self.fixes_dir,
            self.tasks_dir,
        ):
            directory.mkdir(parents=True, exist_ok=True)
        self.team_log = self.logs_dir / "team.log"
        self.task_log = self.logs_dir / ("task-" + self.task_id + ".log")

    def log_line(self, message):
        line = "{} task={} model={} {} -> {} {}\n".format(
            timestamp(),
            self.task_id,
            env("RHEI_MODEL", "none"),
            env("RHEI_FROM_STATE", "unknown"),
            env("RHEI_TO_STATE", "unknown"),
            message,
        )
        for log in (self.team_log, self.task_log):
            with log.open("a", encoding="utf-8", newline="") as handle:
                handle.write(line)

    def review_output_path(self, model):
        return self.findings_dir / (model + "-findings.md")

    # write-review: called once per model (RHEI_MODEL is set by the runtime).
    # Each model writes its findings to runtime/findings/<model>-findings.md.
    def write_review(self):
        model = env("RHEI_MODEL", "unknown")
        self.log_line("wrote review findings")
        output_path = self.review_output_path(model)

        if env("RHEI_LIVING_REVIEW_MODE", "mock") != "live":
            if model not in MOCK_REVIEWS:
                print("unknown model: " + model, file=sys.stderr)
                return 1
            write_file(output_path, MOCK_REVIEWS[model])
            return 0

        if model == "claude":
            with output_path.open("w", encoding="utf-8") as handle:
                subprocess.run(
                    [
                        "claude",
                        "-p",
                        "--output-format",
                        "text",
                        "--permission-mode",
                        "bypassPermissions",
                        "--add-dir",
                        str(self.workspace_root),
                        review_prompt(model),
                    ],
                    stdout=handle,
                    check=True,
                )
            return 0
        if model == "codex":
            subprocess.run(
                [
                    "codex",
                    "exec",
                    "--sandbox",
                    "read-only",
                    "--skip-git-repo-check",
                    "--cd",
                    str(self.workspace_root),
                    "--output-last-message",
                    str(output_path),
                    review_prompt(model),
                ],
                check=True,
            )
            return 0
        print("unknown model: " + model, file=sys.stderr)
        return 1

    # consolidate: called once after all model reviews are written. Merges
    # findings and appends verification tasks to the workspace.
    def consolidate(self):
        self.log_line("consolidated multi-model findings and spawned verification tasks")

        merged = ["# Review Findings\n"]
        for model in ("claude", "codex"):
            merged.append("\n## Model {}\n".format(model))
            source = self.findings_dir / (model + "-findings.md")
            if source.is_file():
                for line in source.read_text(encoding="utf-8").splitlines():
                    if line.startswith("-"):
                        merged.append(line + "\n")
        merged.append("\n## Consolidated review points\n")
        merged.append(
            "1. F-001: Verify whether cache invalidation can cross project boundaries.\n"
        )
        merged.append(
            "2. F-002: Verify whether the stale CLI help text is still reproducible.\n"
        )
        merged.append(
            "3. F-003: Verify whether timeout details are lost during retries.\n"
        )
        with (self.findings_dir / "review-findings.md").open(
            "w", encoding="utf-8", newline=""
        ) as handle:
            handle.write("".join(merged))

        write_task_if_missing(
            self.tasks_dir / "02-verify-cache-key.md",
            "### Task verify-cache-key: Verify and reproduce finding F-001\n"
            "**State:** prove\n"
            "**Prior:** Task review-seed\n"
            "\n"
            "Check whether cache invalidation can reproduce across project boundaries and\n"
            "record whether the finding is relevant enough to justify a fix.",
        )
        write_task_if_missing(
            self.tasks_dir / "03-verify-cli-help.md",
            "### Task verify-cli-help: Verify and reproduce finding F-002\n"
            "**State:** prove\n"
            "**Prior:** Task review-seed\n"
            "\n"
            "Check whether the stale CLI help wording still exists in the current workspace\n"
            "and record whether the finding is relevant to the current scope.",
        )
        write_task_if_missing(
            self.tasks_dir / "04-verify-timeout-details.md",
            "### Task verify-timeout-details: Verify and reproduce finding F-003\n"
            "**State:** prove\n"
            "**Prior:** Task review-seed\n"
            "\n"
            "Check whether retry handling hides the upstream timeout detail and record\n"
            "whether the finding is relevant enough to justify a fix.",
        )
        return 0

    # execute-task: called for verification and fix tasks (pending -> completed).
    def verify_cache_key(self):
        self.log_line("verified F-001 as relevant and spawned a fix task")
        write_file(
            self.verifications_dir / "F-001.md",
            "# Verification F-001\n"
            "\n"
            "- Reproduced: yes\n"
            "- Relevant: yes\n"
            "- Summary: a missing project identifier in the cache key could let one project\n"
            "  observe another project's invalidation behavior.",
        )
        write_task_if_missing(
            self.tasks_dir / "11-fix-cache-key.md",
            "### Task fix-cache-key: Fix finding F-001 after verified reproduction\n"
            "**State:** prove\n"
            "**Prior:** Task verify-cache-key\n"
            "\n"
            "Apply the smallest fix that keeps cache invalidation scoped to one project now\n"
            "that the issue is reproduced and confirmed relevant.",
        )

    def verify_cli_help(self):
        self.log_line("verified F-002 as not relevant and skipped fix expansion")
        write_file(
            self.verifications_dir / "F-002.md",
            "# Verification F-002\n"
            "\n"
            "- Reproduced: no\n"
            "- Relevant: no\n"
            "- Summary: the current workspace no longer contains the stale help text, so the\n"
            "  review note came from an older snapshot and does not justify a fix task.",
        )

    def verify_timeout_details(self):
        self.log_line("verified F-003 as relevant and spawned a fix task")
        write_file(
            self.verifications_dir / "F-003.md",
            "# Verification F-003\n"
            "\n"
            "- Reproduced: yes\n"
            "- Relevant: yes\n"
            "- Summary: retry handling drops the original timeout context, which makes\n"
            "  production diagnosis harder and merits a focused fix.",
        )
        write_task_if_missing(
            self.tasks_dir / "12-fix-timeout-details.md",
            "### Task fix-timeout-details: Fix finding F-003 after verified reproduction\n"
            "**State:** prove\n"
            "**Prior:** Task verify-timeout-details\n"
            "\n"
            "Preserve the upstream timeout detail through the retry path now that the issue\n"
            "is reproduced and confirmed relevant.",
        )

    def fix_cache_key(self):
        self.log_line("completed fix task for F-001")
        write_file(
            self.fixes_dir / "F-001.md",
            "# Fix F-001\n"
            "\n"
            "- Status: completed\n"
            "- Action: include the project identifier in the cache invalidation key.",
        )

    def fix_timeout_details(self):
        self.log_line("completed fix task for F-003")
        write_file(
            self.fixes_dir / "F-003.md",
            "# Fix F-003\n"
            "\n"
            "- Status: completed\n"
            "- Action: preserve the original timeout details when retries exhaust.",
        )

    def execute_task(self):
        # Dispatch on the rhei-local heading id; RHEI_TASK_ID is the
        # project-qualified form (e.g. `living-review-loop.verify-cache-key`).
        local = env("RHEI_TASK_ID_LOCAL")
        handlers = {
            "verify-cache-key": self.verify_cache_key,
            "verify-cli-help": self.verify_cli_help,
            "verify-timeout-details": self.verify_timeout_details,
            "fix-cache-key": self.fix_cache_key,
            "fix-timeout-details": self.fix_timeout_details,
        }
        handler = handlers.get(local)
        if handler is None:
            print("unknown task id for execute-task: " + local, file=sys.stderr)
            return 1
        handler()
        return 0

    def cancel_task(self):
        self.log_line("task cancelled")
        write_file(
            self.runtime_dir / ("cancelled-" + self.task_id + ".txt"),
            "task {} cancelled at {}".format(self.task_id, timestamp()),
        )
        return 0


def main():
    command_name = sys.argv[1] if len(sys.argv) > 1 else ""

    plan_path = env("RHEI_PLAN_PATH")
    if not plan_path:
        print("RHEI_PLAN_PATH is required", file=sys.stderr)
        return 1
    plan_path = pathlib.Path(plan_path)
    workspace_root = plan_path if plan_path.is_dir() else plan_path.parent

    workflow = Workflow(workspace_root)
    if command_name == "write-review":
        return workflow.write_review()
    if command_name == "consolidate":
        return workflow.consolidate()
    if command_name == "execute-task":
        return workflow.execute_task()
    if command_name == "cancel":
        return workflow.cancel_task()
    print("unknown workflow command: " + command_name, file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
