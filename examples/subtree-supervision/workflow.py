"""Mock supervisor and worker for the subtree-supervision example.

Stands in for a real coding agent so the example runs with no credentials:
every state writes the artifact its contract declares, and the `supervising`
state writes the brief the next step reads. `rhei run` does the rest — the
hold/release barrier, the checkpoints, and the edge selection are the engine's.

Python rather than a shell script, so the example runs wherever the CLI runs;
`.agents/rhei/settings.json` execs it directly, with no shell in between.
"""

import os
import pathlib


def env(name, default=''):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    value = os.environ.get(name)
    return value if value else default


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


# Every worker records why its ticket ends where it does; a `final: true` state
# is not entered until that file has content.
def result(text):
    path = env('RHEI_RESULT_PATH')
    if path:
        write(path, text)


# The agent runs with the *repository* as its cwd, so resolve the workspace
# from RHEI_PLAN_PATH the way every callback in these examples does.
root = pathlib.Path(env('RHEI_PLAN_PATH', '.'))
if root.is_file():
    root = root.parent
task = env('RHEI_TASK_ID', 'unknown')
state = env('RHEI_STATE', 'unknown')
visit = env('RHEI_VISIT_COUNT', '1')

runtime = root / 'runtime'
for folder in ('logs', 'review', 'supervise'):
    (runtime / folder).mkdir(parents=True, exist_ok=True)
with (runtime / 'logs' / 'subtree-supervision.log').open('a', encoding='utf-8', newline='') as handle:
    handle.write('task=%s state=%s visit=%s\n' % (task, state, visit))

if state == 'review':
    write(
        runtime / 'review' / (task + '.md'),
        '# Findings for %s\n\n- overflow on a 64-bit literal\n' % task)
    result('## Result\n\nReviewed %s; findings recorded.\n' % task)
elif state == 'fix':
    result('## Result\n\nApplied the briefed fixes for %s.\n' % task)
elif state == 'supervising':
    # One brief per visit, aimed at the child the release edge lets run next.
    # A real supervisor picks the target off `## Checkpoints`; the mock walks
    # the chain in order.
    child = {'1': '1.1', '2': '1.2', '3': '1.3', '4': '1.4'}.get(visit, '')
    if child:
        write(
            runtime / 'supervise' / (task + '.' + child.split('.', 1)[1] + '.md'),
            'Brief from the supervisor (visit %s): stay inside what the review asked for.\n'
            % visit)
    # Only the visit that finds the subtree closed writes a result; on every
    # other visit the engine takes the unconditional self-loop and releases.
    else:
        result(
            '## Result\n\nSupervised %s across %s visits; every child is terminal.\n'
            % (task, visit))
