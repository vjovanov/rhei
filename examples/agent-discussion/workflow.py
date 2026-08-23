"""Callback driver for the agent-discussion example.

By default it writes deterministic, canned positions so the checked-in example
runs in CI without model credentials. Set RHEI_DISCUSSION_MODE=live to dispatch
each participant to a real CLI instead. Set RHEI_DISCUSSION_FORCE_ESCALATE=1 to
make the judge never converge, driving the discussion to the human gate.

The discussion round is taken from the digests already on disk: the judge writes
one at the end of every round, so every participant in a round sees the same
count.

Python rather than a shell script, so the example runs wherever the CLI runs.
`discussion-states.yaml` names it as `cli:python3 ./workflow.py <command>`: a
`cli:` callback goes to the platform's own shell (`sh -c`, `cmd /C`) with the
state machine's directory as its cwd, so the relative path resolves on every
platform and the shell only has to find `python3`. The live-mode branches exec
`claude`, `codex`, `gemini`, and `cursor-agent` directly, never through a shell.
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

# Resolve the workspace root. Prefer RHEI_PLAN_PATH (set by the runtime); fall
# back to this script's own directory so a manual invocation still works.
script_dir = pathlib.Path(__file__).resolve().parent
workspace_root = pathlib.Path(env('RHEI_PLAN_PATH', str(script_dir)))
if workspace_root.is_file():
    workspace_root = workspace_root.parent

runtime_dir = workspace_root / 'runtime'
disc_dir = runtime_dir / 'discussion'
digest_dir = disc_dir / 'digest'
logs_dir = runtime_dir / 'logs'

digest_dir.mkdir(parents=True, exist_ok=True)
logs_dir.mkdir(parents=True, exist_ok=True)

log_file = logs_dir / 'discussion.log'

# The four participants and the project goal each one champions.
PARTICIPANTS = ('claude', 'codex', 'gemini', 'cursor')

GOALS = {
    'claude': 'Developer Experience — keep coordination frictionless and human-legible',
    'codex': 'Determinism & Auditability — every decision must be reproducible and recorded',
    'gemini': 'Throughput & Scale — never put a human in the hot path of a parallel swarm',
    'cursor': 'Safety & Human Oversight — irreversible decisions must have a human gate',
}

# Maximum discussion rounds before the judge escalates to a human.
CAP = 3


def goal_for(model):
    return GOALS.get(model, 'General project health')


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec='seconds')


# The current round number = number of digests already written + 1. The judge
# writes one digest at the end of each round, so every participant in the same
# round sees the same count.
def current_round():
    return len(list(digest_dir.glob('round-*.md'))) + 1


def append(path, text):
    with path.open('a', encoding='utf-8') as handle:
        handle.write(text)


def log_line(message):
    append(log_file, '%s task=%s model=%s %s -> %s %s\n' % (
        timestamp(),
        env('RHEI_TASK_ID', 'unknown'),
        env('RHEI_MODEL', 'none'),
        env('RHEI_FROM_STATE', '?'),
        env('RHEI_TO_STATE', '?'),
        message))


def use_live():
    return env('RHEI_DISCUSSION_MODE', 'mock') == 'live'


def force_escalate():
    return env('RHEI_DISCUSSION_FORCE_ESCALATE', '0') == '1'


# ---------------------------------------------------------------------------
# Mock positions: opening stances in round 1, convergence in round 2.
# ---------------------------------------------------------------------------

MOCK_POSITION_ROUND1 = {
    'claude': """Auto-merge the decision. The plan is the single source of truth and git already
records every change, so a human can always read the diff. Forcing a gate on
*every* decision destroys the frictionless flow that makes a swarm usable.
""",
    'codex': """No silent auto-merge. The judge must record a structured ruling — the decision and
its rationale — so every outcome is reproducible and auditable later. Whether a
human is in the loop matters less than whether the decision is written down.
""",
    'gemini': """Never put a human in the hot path. With many agents running in parallel, a human
gate on each decision serializes the whole swarm and throughput collapses.
Auto-merge, and treat the judge's digest as the durable record.
""",
    'cursor': """Some decisions are irreversible — deleting data, shipping to production, rewriting
history. Those MUST pass a human gate. Auto-merging the irreversible subset is
exactly how an autonomous swarm causes real damage.
""",
}

MOCK_POSITION_ROUND2 = {
    'claude': """codex is right that we need a record — but the judge's per-round digest already is
one, so auditability does not require a gate. I accept cursor's point: gate only
the irreversible subset. Low-risk decisions still auto-merge, so flow is preserved.
""",
    'codex': """Agreed with claude: the per-round digest plus an explicit risk classification
satisfies auditability without blocking. I withdraw the demand to gate everything —
recording the decision and its risk class is enough for reproducibility.
""",
    'gemini': """cursor's gate is acceptable *because* the irreversible subset is rare; the common
low-risk case still auto-merges, so the swarm is not serialized. Throughput is
preserved as long as the judge, not a human, classifies the routine cases.
""",
    'cursor': """If the judge classifies risk and the irreversible subset is gated, I am satisfied.
I concede that low-risk auto-merge is safe as long as the digest records what
happened and the classification is explicit.
""",
}


def write_mock_position(model, round_number, out):
    positions = MOCK_POSITION_ROUND1 if round_number <= 1 else MOCK_POSITION_ROUND2
    out.write_text(
        '# Position — %s (round %s)\n**Champions:** %s\n\n%s'
        % (model, round_number, goal_for(model), positions.get(model, '')),
        encoding='utf-8')


# Live mode: dispatch the participant to a real CLI with a stance-aware prompt.
def write_live_position(model, round_number, out):
    digests = sorted(digest_dir.glob('round-*.md'))
    prior_digest = digests[-1] if digests else None

    prompt = (
        'You are %s, a participant in a structured discussion.\n'
        'You champion this project goal: %s.\n'
        'Argue the point strictly from that goal. This is round %s.\n'
        '\n'
        'The point under discussion:\n'
        'When an agent discussion converges on a decision, how should that decision enter\n'
        'the plan — auto-merge, a recorded judge ruling, or human escalation?\n'
        % (model, goal_for(model), round_number))
    if prior_digest is not None:
        prompt += (
            '\n'
            "Here is the previous round's digest. Respond to the other participants by name,\n"
            'concede what they got right, and sharpen where you still disagree:\n'
            '\n'
            '%s\n' % prior_digest.read_text(encoding='utf-8').rstrip('\n'))
    prompt += '\nRespond with a short markdown position (4-6 sentences).'

    if model == 'claude':
        run_to_file(
            ['claude', '-p', '--output-format', 'text',
             '--permission-mode', 'bypassPermissions'],
            prompt, out)
    elif model == 'codex':
        subprocess.run(
            ['codex', 'exec', '--sandbox', 'danger-full-access',
             '--skip-git-repo-check', '--cd', str(workspace_root),
             '--output-last-message', str(out), '-'],
            input=prompt, text=True, check=True)
    elif model == 'gemini':
        run_to_file(['gemini', '--prompt', '-', '--yolo'], prompt, out)
    elif model == 'cursor':
        run_to_file(['cursor-agent', '--print', '--force'], prompt, out)
    else:
        print('unknown participant for live mode: %s' % model, file=sys.stderr)
        sys.exit(1)


def run_to_file(argv, prompt, out):
    with out.open('w', encoding='utf-8') as handle:
        subprocess.run(argv, input=prompt, text=True, stdout=handle, check=True)


# ---------------------------------------------------------------------------
# Callbacks
# ---------------------------------------------------------------------------

# Fires once per participant (all_models fanout), on leaving a `*-collect` state.
def write_position():
    model = env('RHEI_MODEL', 'unknown')
    round_number = current_round()
    round_dir = disc_dir / ('round-%s' % round_number)
    round_dir.mkdir(parents=True, exist_ok=True)
    out = round_dir / (model + '.md')

    log_line('wrote round %s position' % round_number)

    if use_live():
        write_live_position(model, round_number, out)
    else:
        write_mock_position(model, round_number, out)


# Whether the round reached consensus. The mock converges in round 2 unless
# escalation is forced; live mode asks the judge CLI for a verdict.
def round_converged(round_number):
    if force_escalate():
        return False
    if use_live():
        return live_round_converged(round_number)
    return round_number >= 2


def live_round_converged(round_number):
    positions = sorted((disc_dir / ('round-%s' % round_number)).glob('*.md'))
    corpus = ''.join(path.read_text(encoding='utf-8') for path in positions)
    prompt = (
        'Have these discussion positions converged on a single decision?'
        ' Answer exactly CONVERGED or CONTINUE.\n\n%s\n' % corpus.rstrip('\n'))
    try:
        verdict = subprocess.run(
            ['codex', 'exec', '--sandbox', 'read-only', '--skip-git-repo-check',
             '--output-last-message', '/dev/stdout', '-'],
            input=prompt, text=True, capture_output=True).stdout
    except OSError:
        verdict = ''
    return 'CONVERGED' in verdict


DECISION = """# Decision: D-merge-policy
**Converged:** round %s at %s
**Participants:** %s

## Question
When an agent discussion converges on a decision, how should that decision enter
the plan — auto-merge, a recorded judge ruling, or human escalation?

## Decision (risk-tiered)
- Low-risk decisions auto-merge into the plan; the judge's per-round digest is the
  recorded audit trail. (Honors Developer Experience, Throughput, and Auditability.)
- Decisions the judge classifies as irreversible or destructive escalate to a human
  review gate before they take effect. (Honors Safety & Human Oversight.)
- The judge classifies and records each decision's risk class.

## How the competing goals were reconciled
- Determinism & Auditability: met by recording the digest + risk class, not by
  blocking every decision.
- Safety & Human Oversight: met by gating only the irreversible subset.
- Developer Experience & Throughput: met by auto-merging the common low-risk case
  with no human in the hot path.
"""


def write_decision(round_number):
    (disc_dir / 'decision.md').write_text(
        DECISION % (round_number, timestamp(), ' '.join(PARTICIPANTS)),
        encoding='utf-8')


# Fires once on leaving `judge`. Writes the round digest, then redirects via
# `nextState`: `converged` (consensus), `escalated` (round budget spent), or no
# redirect so the engine takes the declared default transition (judge -> collect).
def judge_round():
    round_number = current_round()
    digest = digest_dir / ('round-%s.md' % round_number)

    parts = [
        '# Discussion digest — round %s\n\n' % round_number,
        'Point: how should a converged discussion enter the plan?\n\n',
    ]
    for participant in PARTICIPANTS:
        position = disc_dir / ('round-%s' % round_number) / (participant + '.md')
        if position.is_file():
            parts.append('## %s — %s\n\n' % (participant, goal_for(participant)))
            parts.append(position.read_text(encoding='utf-8'))
            parts.append('\n')
    digest.write_text(''.join(parts), encoding='utf-8')

    if round_converged(round_number):
        write_decision(round_number)
        append(digest, '\n## Outcome\nConverged on a risk-tiered merge policy.'
                       ' Decision recorded in decision.md.\n')
        log_line('round %s converged -> decision recorded' % round_number)
        print('{"success": true, "nextState": "converged"}')
        return

    if round_number >= CAP:
        append(digest, '\n## Outcome\nRound budget (%s) exhausted without consensus.'
                       ' Escalating to a human.\n' % CAP)
        log_line('round %s exhausted budget -> escalating' % round_number)
        print('{"success": true, "nextState": "escalated"}')
        return

    append(digest, '\n## Outcome\nNo consensus yet — the safety/oversight axis and the'
                   ' throughput/DX axis are still in tension. Opening another round.\n')
    log_line('round %s inconclusive -> another round' % round_number)
    # No redirect: the engine applies the declared default transition (judge -> collect).
    print('{"success": true}')


APPLIED = """# Applied: D-merge-policy
**Applied:** %s

The converged merge policy is now in effect:
- low-risk decisions auto-merge, with the judge digest as the audit trail
- irreversible decisions escalate to a human review gate

See runtime/discussion/decision.md for the full ruling.
"""


# Fires on leaving `apply` (the downstream task that depends on the decision).
def apply_decision():
    decision = disc_dir / 'decision.md'
    if not decision.is_file():
        print('decision.md not found; the discussion has not converged', file=sys.stderr)
        sys.exit(1)
    log_line('applied the converged decision')
    (disc_dir / 'applied.md').write_text(APPLIED % timestamp(), encoding='utf-8')


def cancel_task():
    log_line('discussion cancelled')
    (disc_dir / ('cancelled-%s.txt' % env('RHEI_TASK_ID', 'unknown'))).write_text(
        'cancelled at %s\n' % timestamp(), encoding='utf-8')


if command_name == 'write-position':
    write_position()
elif command_name == 'judge-round':
    judge_round()
elif command_name == 'apply-decision':
    apply_decision()
elif command_name == 'cancel':
    cancel_task()
else:
    print('unknown workflow command: %s' % command_name, file=sys.stderr)
    sys.exit(1)
