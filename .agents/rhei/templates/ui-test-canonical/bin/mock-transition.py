"""Mock transition callback for the canonical UI fixture.

Python rather than a shell script, so the callbacks run wherever the CLI runs.
`states.yaml` names it as `cli:python3 ./bin/mock-transition.py <command>`: a
`cli:` callback goes to the platform's own shell (`sh -c`, `cmd /C`) with the
state machine's directory as its cwd, so the relative path resolves on every
platform and the shell only has to find `python3`.
"""

import datetime
import os
import pathlib
import re
import sys


def env(name, default=''):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    value = os.environ.get(name)
    return value if value else default


command_name = sys.argv[1] if len(sys.argv) > 1 else 'log'

plan_path = env('RHEI_PLAN_PATH')
if not plan_path:
    print('RHEI_PLAN_PATH is required', file=sys.stderr)
    sys.exit(1)

plan_path = pathlib.Path(plan_path)
if plan_path.is_dir():
    workspace_root = plan_path
    plan_file = workspace_root / 'index.rhei.md'
    tasks_dir = workspace_root / 'tasks'
else:
    workspace_root = plan_path.parent
    plan_file = plan_path
    tasks_dir = None

task_id = env('RHEI_TASK_ID', 'unknown')
from_state = env('RHEI_FROM_STATE', 'unknown')
to_state = env('RHEI_TO_STATE', 'unknown')
transition_root = workspace_root / 'runtime' / 'transitions'
include_generated = '{{ include_generated_followup }}'

transition_root.mkdir(parents=True, exist_ok=True)

FOLLOWUP_BODY = (
    '### Task generated-followup-%s: Generated follow-up for %s\n'
    '**State:** script-check\n'
    '\n'
    'This task was appended by the aggregate transition callback so the UI can show\n'
    'workspace expansion during a live run.\n'
)


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec='seconds')


def safe_task_id():
    return re.sub(r'[^A-Za-z0-9_-]', '-', task_id)


def append_transition(line):
    with (transition_root / 'transitions.log').open('a', encoding='utf-8', newline='') as handle:
        handle.write(line)


def log_transition():
    append_transition('%s task=%s %s -> %s command=%s\n' % (
        timestamp(), task_id, from_state, to_state, command_name))


def append_generated_followup():
    if include_generated != 'true':
        return

    safe_id = safe_task_id()
    marker = 'Task generated-followup-%s:' % safe_id
    body = FOLLOWUP_BODY % (safe_id, task_id)
    if tasks_dir is not None:
        tasks_dir.mkdir(parents=True, exist_ok=True)
        followup = tasks_dir / ('99-generated-followup-' + safe_id + '.md')
        if followup.exists():
            return
        with followup.open('w', encoding='utf-8', newline='') as handle:
            handle.write(body)
        return

    if marker in plan_file.read_text(encoding='utf-8'):
        return

    with plan_file.open('a', encoding='utf-8', newline='') as handle:
        handle.write('\n' + body)


if command_name == 'log':
    log_transition()
elif command_name == 'enter':
    append_transition('%s task=%s entered %s (from %s) command=%s\n' % (
        timestamp(), task_id, to_state, from_state, command_name))
elif command_name == 'aggregate':
    log_transition()
    append_generated_followup()
else:
    print('unknown transition command: %s' % command_name, file=sys.stderr)
    sys.exit(1)
