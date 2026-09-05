"""Mock program for the canonical UI fixture.

Python rather than a shell script, so `rhei run` drives this workspace on every
platform the CLI ships to. `states.yaml` execs it as
`["python3", "./bin/mock-program.py", "<command>"]`: a program's cwd *is* the
workspace root, so the relative path resolves and no shell is involved.
"""

import datetime
import os
import pathlib
import sys
import time


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

task_id = env('RHEI_TASK_ID', 'unknown')
state = env('RHEI_STATE', 'unknown')
visit_count = env('RHEI_VISIT_COUNT', '1')
step_delay = env('MOCK_NODE_DELAY_SECONDS', '{{ step_delay_seconds }}')

runtime = workspace_root / 'runtime'
log_path = runtime / 'logs' / 'mock-program.log'
log_path.parent.mkdir(parents=True, exist_ok=True)


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec='seconds')


def write_file(path, content):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(content + '\n')


def append_log(message):
    line = '%s task=%s state=%s visit=%s command=%s %s\n' % (
        timestamp(), task_id, state, visit_count, command_name, message)
    with log_path.open('a', encoding='utf-8', newline='') as handle:
        handle.write(line)


# A worker records why the ticket ends where it does: a `final: true` state is
# not entered until RHEI_RESULT_PATH has content, so a program whose exit can
# route the ticket into one must write it. §FS-rhei-states.3.3 §FS-rhei-programs.2
def write_result():
    result_path = env('RHEI_RESULT_PATH')
    if not result_path:
        return
    write_file(
        result_path,
        '## Result\n\nMock program `%s` finished task %s in state %s.'
        % (command_name, task_id, state))


time.sleep(float(step_delay))
append_log('started')

scenario = env('MOCK_SCENARIO', 'unknown')

if command_name == 'normalize':
    artifact_dir = runtime / 'artifacts' / task_id
    write_file(
        artifact_dir / 'normalized.json',
        '{"task":"%s","scenario":"%s","normalized":true}' % (task_id, scenario))
    write_file(artifact_dir / 'io-map.md', '\n'.join([
        '# IO Map %s' % task_id,
        '',
        '- input: %s' % env(
            'RHEI_INPUT_RAW_INPUTS_PATH', 'runtime/artifacts/%s/inputs.md' % task_id),
        '- notes: %s' % env(
            'RHEI_INPUT_RAW_NOTES_PATH', 'runtime/artifacts/%s/notes.json' % task_id),
        '- output: runtime/artifacts/%s/normalized.json' % task_id,
    ]))
elif command_name == 'build':
    write_file(runtime / 'build' / (task_id + '-report.md'), '\n'.join([
        '# Build Report %s' % task_id,
        '',
        '- implementation: %s' % env(
            'RHEI_INPUT_IMPLEMENTATION_PATH', 'runtime/implementation/%s.md' % task_id),
        '- scenario: %s' % scenario,
        '- status: passed',
    ]))
    write_file(runtime / 'build' / (task_id + '-bundle.txt'), 'bundle for %s' % task_id)
elif command_name == 'aggregate':
    reviews_dir = runtime / 'reviews'
    reviews = sorted(
        path.relative_to(workspace_root).as_posix()
        for path in reviews_dir.glob(task_id + '-*.md')
        if path.is_file()
    ) if reviews_dir.is_dir() else []
    lines = [
        '# Aggregate %s' % task_id,
        '',
        '- scenario: %s' % scenario,
        '- review files:',
    ]
    lines.extend('  - %s' % review for review in reviews)
    write_file(runtime / 'aggregate' / (task_id + '.md'), '\n'.join(lines))
    write_file(
        runtime / 'aggregate' / (task_id + '.json'),
        '{"task":"%s","aggregated":true}' % task_id)
elif command_name == 'poll':
    if int(visit_count) < int('{{ poll_attempts }}'):
        write_file(runtime / 'poll' / (task_id + '-attempt-' + visit_count + '.md'), '\n'.join([
            '# Poll Attempt %s' % visit_count,
            '',
            'Mock external system still running for %s.' % task_id,
        ]))
        append_log('poll pending')
        sys.exit(75)
    write_file(
        runtime / 'poll' / (task_id + '-ready.json'),
        '{"task":"%s","ready":true,"attempt":%s}' % (task_id, visit_count))
elif command_name == 'check':
    write_file(runtime / 'checks' / (task_id + '.md'), '\n'.join([
        '# Check %s' % task_id,
        '',
        '- state: %s' % state,
        '- scenario: %s' % scenario,
        '- status: passed',
    ]))
elif command_name == 'fail':
    write_file(runtime / 'failures' / (task_id + '.md'), '\n'.join([
        '# Failure %s' % task_id,
        '',
        '- state: %s' % state,
        '- scenario: %s' % scenario,
        '- status: failed (deterministic mock failure for UI testing)',
    ]))
    append_log('failed exit=42')
    sys.exit(42)
elif command_name == 'poll-exhaust':
    write_file(
        runtime / 'poll' / (task_id + '-pending.json'),
        '{"task":"%s","ready":false,"attempt":%s}' % (task_id, visit_count))
    append_log('poll never ready')
    sys.exit(75)
else:
    print('unknown mock program command: %s' % command_name, file=sys.stderr)
    sys.exit(1)

write_result()
append_log('completed')
