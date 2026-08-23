"""The living review loop's workflow, driven from the machine's `cli:` callbacks.

Python rather than a shell script, so the example runs wherever `python3` is
on `PATH`.
`team-states.yaml` names it as `cli:python3 ./workflow.py <command>`: a `cli:`
callback goes to the platform's own shell (`sh -c`, `cmd /C`) with the state
machine's directory as its cwd, so the relative path resolves on every platform
and the shell only has to find `python3`. The live-reviewer branches exec
`claude` and `codex` directly, never through a shell.
"""

import datetime
import os
import pathlib
import subprocess
import sys


def env(name, default=''):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    value = os.environ.get(name)
    return value if value else default


command_name = sys.argv[1] if len(sys.argv) > 1 else ''

plan_path = env('RHEI_PLAN_PATH')
if not plan_path:
    print('RHEI_PLAN_PATH is required', file=sys.stderr)
    sys.exit(1)

plan_path = pathlib.Path(plan_path)
workspace_root = plan_path if plan_path.is_dir() else plan_path.parent

runtime_dir = workspace_root / 'runtime'
logs_dir = runtime_dir / 'logs'
findings_dir = runtime_dir / 'findings'
verifications_dir = runtime_dir / 'verifications'
fixes_dir = runtime_dir / 'fixes'
tasks_dir = workspace_root / 'tasks'

for directory in (logs_dir, findings_dir, verifications_dir, fixes_dir, tasks_dir):
    directory.mkdir(parents=True, exist_ok=True)

team_log = logs_dir / 'team.log'
task_log = logs_dir / ('task-%s.log' % env('RHEI_TASK_ID', 'unknown'))


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec='seconds')


def log_line(message):
    line = '%s task=%s model=%s %s -> %s %s\n' % (
        timestamp(),
        env('RHEI_TASK_ID', 'unknown'),
        env('RHEI_MODEL', 'none'),
        env('RHEI_FROM_STATE', 'unknown'),
        env('RHEI_TO_STATE', 'unknown'),
        message)
    for log in (team_log, task_log):
        with log.open('a', encoding='utf-8') as handle:
            handle.write(line)


def write_file(path, content):
    path.write_text(content + '\n', encoding='utf-8')


def write_task_if_missing(path, content):
    if path.exists():
        return
    write_file(path, content)


def review_output_path(model):
    return findings_dir / ('%s-findings.md' % model)


# The reviewers read the real repository, which is not the copied workspace.
def review_source_root():
    override = env('RHEI_LIVING_REVIEW_SOURCE_ROOT')
    if override:
        return pathlib.Path(override)

    try:
        git = subprocess.run(
            ['git', '-C', str(workspace_root), 'rev-parse', '--show-toplevel'],
            capture_output=True, text=True)
    except OSError:
        return workspace_root
    if git.returncode == 0:
        return pathlib.Path(git.stdout.strip())

    return workspace_root


def review_specs_root():
    return review_source_root() / 'docs' / 'functional-spec'


def ensure_review_source_root():
    root = review_source_root()
    if review_specs_root().is_dir():
        return

    print('live review requires access to the repository docs/functional-spec directory.\n'
          'Set RHEI_LIVING_REVIEW_SOURCE_ROOT to the project root before running the'
          ' copied workspace.\n'
          'Current review source root: %s' % root, file=sys.stderr)
    sys.exit(1)


def review_specs_manifest():
    return sorted(
        './' + entry.name for entry in review_specs_root().iterdir() if entry.is_file())


def render_review_corpus():
    specs_root = review_specs_root()
    parts = []
    for relpath in review_specs_manifest():
        path = relpath[2:] if relpath.startswith('./') else relpath
        parts.append('\n## FILE: %s\n' % path)
        content = (specs_root / path).read_text(encoding='utf-8')
        parts.append(''.join(content.splitlines(keepends=True)[:4000]))
        parts.append('\n')
    return ''.join(parts)


def review_prompt(model):
    return (
        'Review the following Rhei specification documents for gaps, contradictions,'
        ' and ambiguities.\n'
        'Focus on problems that would mislead an implementor or confuse a user.\n'
        '\n'
        'Files to review:\n'
        '%s\n'
        '\n'
        'Respond with markdown only in this exact shape:\n'
        '# Review Findings: Model %s\n'
        '\n'
        '- F-...: ...\n'
        '- F-...: ...\n'
        '\n'
        'Keep the findings concise and distinct from the other reviewer.\n'
        '\n'
        'Here are the spec files:\n'
        '%s\n'
        % ('\n'.join(review_specs_manifest()), model, render_review_corpus().rstrip('\n')))


def use_live_reviewers():
    return env('RHEI_LIVING_REVIEW_MODE', 'mock') == 'live'


MOCK_REVIEWS = {
    'claude': """# Review Findings: Model claude

- F-001: cache invalidation key appears to omit the project identifier
- F-002: release example help text may still mention a stale flag""",
    'codex': """# Review Findings: Model codex

- F-001: cache key composition looks incomplete around project scoping
- F-003: retry path may swallow the upstream timeout detail""",
}


def write_mock_review(model):
    if model not in MOCK_REVIEWS:
        print('unknown model: %s' % model, file=sys.stderr)
        sys.exit(1)
    write_file(review_output_path(model), MOCK_REVIEWS[model])


def run_claude_review(model):
    output_path = review_output_path(model)

    ensure_review_source_root()

    with output_path.open('w', encoding='utf-8') as handle:
        subprocess.run(
            ['claude', '-p',
             '--output-format', 'text',
             '--permission-mode', 'bypassPermissions'],
            input=review_prompt(model), text=True, stdout=handle, check=True)


def run_codex_review(model):
    output_path = review_output_path(model)

    ensure_review_source_root()

    subprocess.run(
        ['codex', 'exec',
         '--sandbox', 'danger-full-access',
         '--skip-git-repo-check',
         '--cd', str(workspace_root),
         '--add-dir', str(workspace_root),
         '--output-last-message', str(output_path),
         '-'],
        input=review_prompt(model), text=True, check=True)


# write-review: called once per model (RHEI_MODEL is set by the runtime).
# Each model writes its findings to runtime/findings/<model>-findings.md.
def write_review():
    model = env('RHEI_MODEL', 'unknown')
    log_line('wrote review findings')

    if not use_live_reviewers():
        write_mock_review(model)
        return

    if model == 'claude':
        run_claude_review(model)
    elif model == 'codex':
        run_codex_review(model)
    else:
        print('unknown model: %s' % model, file=sys.stderr)
        sys.exit(1)


# consolidate: called once after all model reviews are written. Merges findings
# and appends one verification task per consolidated review point.
def consolidate():
    log_line('consolidated multi-model findings and spawned verification tasks')

    merged = ['# Review Findings\n']
    for model in ('claude', 'codex'):
        merged.append('\n## Model %s\n' % model)
        source = findings_dir / ('%s-findings.md' % model)
        if source.is_file():
            for line in source.read_text(encoding='utf-8').splitlines():
                if line.startswith('-'):
                    merged.append(line + '\n')
    merged.append('\n## Consolidated review points\n')
    merged.append('1. F-001: Verify whether cache invalidation can cross project boundaries.\n')
    merged.append('2. F-002: Verify whether the stale CLI help text is still reproducible.\n')
    merged.append('3. F-003: Verify whether timeout details are lost during retries.\n')
    (findings_dir / 'review-findings.md').write_text(''.join(merged), encoding='utf-8')

    write_task_if_missing(
        tasks_dir / '02-verify-cache-key.md',
        """### Task verify-cache-key: Verify and reproduce finding F-001
**State:** prove
**Prior:** Task review-seed

Check whether cache invalidation can reproduce across project boundaries and
record whether the finding is relevant enough to justify a fix.""")

    write_task_if_missing(
        tasks_dir / '03-verify-cli-help.md',
        """### Task verify-cli-help: Verify and reproduce finding F-002
**State:** prove
**Prior:** Task review-seed

Check whether the stale CLI help wording still exists in the current workspace
and record whether the finding is relevant to the current scope.""")

    write_task_if_missing(
        tasks_dir / '04-verify-timeout-details.md',
        """### Task verify-timeout-details: Verify and reproduce finding F-003
**State:** prove
**Prior:** Task review-seed

Check whether retry handling hides the upstream timeout detail and record
whether the finding is relevant enough to justify a fix.""")


def verify_cache_key():
    log_line('verified F-001 as relevant and spawned a fix task')

    write_file(
        verifications_dir / 'F-001.md',
        """# Verification F-001

- Reproduced: yes
- Relevant: yes
- Summary: a missing project identifier in the cache key could let one project
  observe another project's invalidation behavior.""")

    write_task_if_missing(
        tasks_dir / '11-fix-cache-key.md',
        """### Task fix-cache-key: Fix finding F-001 after verified reproduction
**State:** prove
**Prior:** Task verify-cache-key

Apply the smallest fix that keeps cache invalidation scoped to one project now
that the issue is reproduced and confirmed relevant.""")


def verify_cli_help():
    log_line('verified F-002 as not relevant and skipped fix expansion')

    write_file(
        verifications_dir / 'F-002.md',
        """# Verification F-002

- Reproduced: no
- Relevant: no
- Summary: the current workspace no longer contains the stale help text, so the
  review note came from an older snapshot and does not justify a fix task.""")


def verify_timeout_details():
    log_line('verified F-003 as relevant and spawned a fix task')

    write_file(
        verifications_dir / 'F-003.md',
        """# Verification F-003

- Reproduced: yes
- Relevant: yes
- Summary: retry handling drops the original timeout context, which makes
  production diagnosis harder and merits a focused fix.""")

    write_task_if_missing(
        tasks_dir / '12-fix-timeout-details.md',
        """### Task fix-timeout-details: Fix finding F-003 after verified reproduction
**State:** prove
**Prior:** Task verify-timeout-details

Preserve the upstream timeout detail through the retry path now that the issue
is reproduced and confirmed relevant.""")


def fix_cache_key():
    log_line('completed fix task for F-001')

    write_file(
        fixes_dir / 'F-001.md',
        """# Fix F-001

- Status: completed
- Action: include the project identifier in the cache invalidation key.""")


def fix_timeout_details():
    log_line('completed fix task for F-003')

    write_file(
        fixes_dir / 'F-003.md',
        """# Fix F-003

- Status: completed
- Action: preserve the original timeout details when retries exhaust.""")


# execute-task: called for verification and fix tasks (pending -> completed).
def execute_task():
    # Dispatch on the rhei-local heading id; RHEI_TASK_ID is the
    # project-qualified form (e.g. `living-review-loop.verify-cache-key`).
    local = env('RHEI_TASK_ID_LOCAL')
    handlers = {
        'verify-cache-key': verify_cache_key,
        'verify-cli-help': verify_cli_help,
        'verify-timeout-details': verify_timeout_details,
        'fix-cache-key': fix_cache_key,
        'fix-timeout-details': fix_timeout_details,
    }
    handler = handlers.get(local)
    if handler is None:
        print('unknown task id for execute-task: %s' % local, file=sys.stderr)
        sys.exit(1)
    handler()


def cancel_task():
    log_line('task cancelled')
    task_id = env('RHEI_TASK_ID', 'unknown')
    write_file(
        runtime_dir / ('cancelled-%s.txt' % task_id),
        'task %s cancelled at %s' % (task_id, timestamp()))


if command_name == 'write-review':
    write_review()
elif command_name == 'consolidate':
    consolidate()
elif command_name == 'execute-task':
    execute_task()
elif command_name == 'cancel':
    cancel_task()
else:
    print('unknown workflow command: %s' % command_name, file=sys.stderr)
    sys.exit(1)
