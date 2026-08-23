"""Mock agent for the canonical UI fixture.

Python rather than a shell script: this workspace is meant to run wherever the
CLI runs, and a Windows runner has no `bash`. Every path below is built a
segment at a time with `pathlib`, never by pasting a separator into a string,
so the fixture spells no platform's dialect.

`.agents/rhei/settings.json` spawns this file as

    python3 -c "...runpy.run_path(RHEI_ROOT/bin/mock-agent.py)..."

rather than as `python3 bin/mock-agent.py`, because an agent's cwd is the
repository checkout, not the workspace, so a relative script path would not
resolve. `RHEI_ROOT` is the workspace, and the bootstrap turns it into an
absolute path before Python opens anything.
"""

import datetime
import os
import pathlib
import sys
import time

# Rhei speaks UTF-8 with one `\n` per line on every platform, and Python does
# not: on Windows it decodes stdin in the host's code page and writes `\r\n`.
for _stream in (sys.stdin, sys.stdout, sys.stderr):
    if hasattr(_stream, 'reconfigure'):
        _stream.reconfigure(encoding='utf-8', newline='')


def env(name, default=''):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    value = os.environ.get(name)
    return value if value else default


prompt = ''
mode = 'default'
model = env('RHEI_MODEL_NAME', env('RHEI_MODEL', 'mock-model'))
session_dir = ''
skills = []

argv = sys.argv[1:]
index = 0
while index < len(argv):
    flag = argv[index]
    value = argv[index + 1] if index + 1 < len(argv) else ''
    if flag == '--prompt':
        prompt = value
        index += 2
    elif flag == '--model':
        model = value or model
        index += 2
    elif flag == '--mode':
        mode = value or mode
        index += 2
    elif flag == '--skill':
        skills.append(value)
        index += 2
    elif flag == '--session-dir':
        session_dir = value
        index += 2
    elif flag in ('--resume', '--fork'):
        index += 2
    elif flag == '--':
        index += 1
        if not prompt:
            prompt = sys.stdin.read()
    else:
        index += 1

if not prompt and sys.stdin is not None and not sys.stdin.isatty():
    prompt = sys.stdin.read()

plan_path = env('RHEI_PLAN_PATH')
if not plan_path:
    print('RHEI_PLAN_PATH is required', file=sys.stderr)
    sys.exit(1)

plan_path = pathlib.Path(plan_path)
workspace_root = plan_path if plan_path.is_dir() else plan_path.parent

task_id = env('RHEI_TASK_ID', 'unknown')
state = env('RHEI_STATE', 'unknown')
target_slug = env('RHEI_TARGET_SLUG', env('RHEI_AGENT', 'mock-agent') + '-' + model)
step_delay = env('MOCK_NODE_DELAY_SECONDS', '0.1')

runtime = workspace_root / 'runtime'
log_path = runtime / 'logs' / 'mock-agent.log'
log_path.parent.mkdir(parents=True, exist_ok=True)


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec='seconds')


def write_file(path, content):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(content + '\n')


def append_log(message):
    line = '%s task=%s state=%s target=%s mode=%s model=%s %s\n' % (
        timestamp(), task_id, state, target_slug, mode, model, message)
    with log_path.open('a', encoding='utf-8', newline='') as handle:
        handle.write(line)


# A worker records why the ticket ends where it does: RHEI_RESULT_PATH names
# this invocation's own fragment, and a `final: true` state is not entered
# until the ticket's result has content. §FS-rhei-states.3.3 §FS-rhei-agents.4
def write_result():
    result_path = env('RHEI_RESULT_PATH')
    if not result_path:
        return
    write_file(
        result_path,
        '## Result\n\nMock agent %s finished task %s in state %s.' % (target_slug, task_id, state))


def write_snapshot_transcript():
    directory = session_dir or env('RHEI_SNAPSHOT_SESSION_DIR')
    if not directory:
        return
    directory = pathlib.Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    transcript = '\n'.join([
        '{"session_id":"mock-%s-%s-%s","provider":"%s","model":"%s"}'
        % (task_id, state, target_slug, env('RHEI_MODEL_PROVIDER', 'mock'), model),
        '{"role":"user","content":"%s prompt for %s"}' % (state, task_id),
        '{"role":"assistant","content":"mock %s output for %s"}' % (state, task_id),
    ])
    with (directory / 'mock-session.jsonl').open('w', encoding='utf-8', newline='') as handle:
        handle.write(transcript + '\n')


time.sleep(float(step_delay))
append_log('started')
write_snapshot_transcript()
prompt_bytes = len(prompt.encode('utf-8'))

if state == 'collect-inputs':
    artifact_dir = runtime / 'artifacts' / task_id
    write_file(artifact_dir / 'inputs.md', '\n'.join([
        '# Inputs for %s' % task_id,
        '',
        '- scenario: dashboard checkout flow',
        '- target: %s' % target_slug,
        '- prompt-bytes: %s' % prompt_bytes,
        '- generated-by: mock-agent',
    ]))
    write_file(
        artifact_dir / 'notes.json',
        '{"task":"%s","state":"%s","target":"%s","scenario":"dashboard checkout flow"}'
        % (task_id, state, target_slug))
elif state == 'mock-implement':
    write_file(runtime / 'implementation' / (task_id + '.md'), '\n'.join([
        '# Implementation %s' % task_id,
        '',
        '- target: %s' % target_slug,
        '- model: %s' % model,
        '- mode: %s' % mode,
        '- skills: %s' % (' '.join(skills) or 'none'),
        '- normalized input consumed: yes',
        '- snapshot session: %s' % (session_dir or env('RHEI_SNAPSHOT_SESSION_DIR', 'none')),
    ]))
elif state == 'parallel-review':
    write_file(runtime / 'reviews' / (task_id + '-' + target_slug + '.md'), '\n'.join([
        '# Review %s %s' % (task_id, target_slug),
        '',
        '- finding: %s accepts the deterministic fixture output.' % target_slug,
        '- build report consumed: yes',
        '- recommendation: continue to aggregate.',
    ]))
elif state == 'fix-loop':
    # Prefer the runtime-provided visit counter. When the runtime does not
    # expose it to agents, fall back to counting the fix artifacts already
    # on disk so the file we write matches the declared `{visit_count}`
    # output path (otherwise the counted loop never satisfies its contract).
    visit = env('RHEI_VISIT_COUNT')
    if not visit:
        fixes_dir = runtime / 'fixes'
        fixes_dir.mkdir(parents=True, exist_ok=True)
        existing = [
            path for path in fixes_dir.glob(task_id + '-visit-*.md') if path.is_file()
        ]
        visit = str(len(existing) + 1)
    write_file(runtime / 'fixes' / (task_id + '-visit-' + visit + '.md'), '\n'.join([
        '# Fix Loop %s Visit %s' % (task_id, visit),
        '',
        '- target: %s' % target_slug,
        '- inherited snapshot parent: %s' % env('RHEI_SNAPSHOT_PARENT_REF', 'none'),
        '- aggregate consumed: yes',
        '- action: deterministic fix note for UI testing.',
    ]))
elif state == 'inherit-ancestor':
    parent_ref = env('RHEI_SNAPSHOT_PARENT_REF')
    inherited = 'preloaded' + parent_ref if parent_ref else 'absent (continuing without it)'
    write_file(runtime / 'inherit' / (task_id + '.md'), '\n'.join([
        '# Ancestor Inheritance %s' % task_id,
        '',
        '- target: %s' % target_slug,
        '- inherited snapshot parent: %s' % (parent_ref or 'none'),
        '- result: ancestor implementation snapshot %s' % inherited,
    ]))
else:
    write_file(runtime / 'artifacts' / task_id / (state + '-agent.md'), '\n'.join([
        '# Mock Agent Output',
        '',
        '- task: %s' % task_id,
        '- state: %s' % state,
        '- target: %s' % target_slug,
    ]))

write_result()
append_log('completed')
print('mock agent completed task=%s state=%s target=%s' % (task_id, state, target_slug))
