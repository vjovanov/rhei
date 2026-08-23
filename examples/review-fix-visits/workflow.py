"""Callbacks for the counted review/fix loop in the review-fix-visits example.

Python rather than a shell script, so the example runs wherever `python3` is
on `PATH`.
`states.yaml` names it as `cli:python3 ./workflow.py <command>`: a `cli:`
callback goes to the platform's own shell (`sh -c`, `cmd /C`) with the state
machine's directory as its cwd, so the relative path resolves on every platform
and the shell only has to find `python3`.
"""

import datetime
import os
import pathlib
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
reviews_dir = runtime_dir / 'reviews'
fixes_dir = runtime_dir / 'fixes'

for directory in (logs_dir, reviews_dir, fixes_dir):
    directory.mkdir(parents=True, exist_ok=True)

task_id = env('RHEI_TASK_ID', 'unknown')
team_log = logs_dir / 'team.log'
task_log = logs_dir / ('task-%s.log' % task_id)
review_file = reviews_dir / ('task-%s-review.md' % task_id)
fix_file = fixes_dir / ('task-%s-fix.md' % task_id)


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec='seconds')


def log_line(message):
    line = '%s task=%s %s -> %s %s\n' % (
        timestamp(),
        task_id,
        env('RHEI_FROM_STATE', 'unknown'),
        env('RHEI_TO_STATE', 'unknown'),
        message)
    for log in (team_log, task_log):
        with log.open('a', encoding='utf-8') as handle:
            handle.write(line)


def review_pass_count():
    if not review_file.is_file():
        return 0

    return sum(
        1 for line in review_file.read_text(encoding='utf-8').splitlines()
        if line.startswith('## Review pass '))


def append(path, text):
    with path.open('a', encoding='utf-8') as handle:
        handle.write(text)


def append_review():
    next_pass = review_pass_count() + 1

    log_line('appended review pass %s' % next_pass)

    if not review_file.is_file():
        review_file.write_text(
            '# Review Artifact for Task %s\n'
            '\n'
            'This file is appended once per exit from the counted `review` state.\n'
            % task_id,
            encoding='utf-8')

    append(review_file,
           '\n'
           '## Review pass %s\n'
           '\n'
           '- Transition: %s -> %s\n'
           '- Observation: review pass %s captured a concrete finding for the fix step.\n'
           '- Output file: runtime/reviews/task-%s-review.md\n'
           % (next_pass,
              env('RHEI_FROM_STATE', 'unknown'),
              env('RHEI_TO_STATE', 'unknown'),
              next_pass,
              task_id))


def write_fix():
    log_line('updated fix artifact from review file')

    if not review_file.is_file():
        print('missing review artifact for task %s' % task_id, file=sys.stderr)
        sys.exit(1)

    pass_count = review_pass_count()
    if pass_count < 1 or pass_count > 2:
        print('expected 1 or 2 review passes, found %s' % pass_count, file=sys.stderr)
        sys.exit(1)

    fix_file.write_text(
        '# Fix Artifact for Task %s\n'
        '\n'
        'Source artifact: runtime/reviews/task-%s-review.md\n'
        'Review passes consumed: %s\n'
        '\n'
        '## Applied fix\n'
        '\n'
        '- Read the shared review artifact.\n'
        '- %s review pass(es) were available when this fix step ran.\n'
        '- Produced the current fix artifact revision from the accumulated review findings.\n'
        % (task_id, task_id, pass_count, pass_count),
        encoding='utf-8')


def cancel_task():
    log_line('task cancelled')
    fix_file.write_text(
        '# Cancelled Task %s\n'
        '\n'
        'The workflow stopped before completion at %s.\n'
        % (task_id, timestamp()),
        encoding='utf-8')


if command_name == 'append-review':
    append_review()
elif command_name == 'write-fix':
    write_fix()
elif command_name == 'cancel':
    cancel_task()
else:
    print('unknown workflow command: %s' % command_name, file=sys.stderr)
    sys.exit(1)
