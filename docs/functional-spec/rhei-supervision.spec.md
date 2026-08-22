# FS-rhei-supervision: Subtree Supervision Specification

This document specifies **subtree supervision**: how a non-leaf task node
looks after the tasks beneath it *while* they run, instead of only
integrating them once they are all finished. A state that declares
`supervise:` turns the task holding it into a *supervisor*. The orchestrator
wakes the supervisor at *checkpoints* — after every descendant finishes
(`task`) or after every state a descendant passes through (`state`) — with
the same agent session continued from its previous visit, and holds the rest
of the subtree while the supervisor decides how to steer it. §GOAL-rhei-outcomes

Supervision builds on the non-leaf task model (§FS-rhei-plan-language.3): a
parent is a task in its own right, and a parent and one of its descendants are
never worked at the same time. Supervision does not relax that. It adds the
one thing the model lacked — a parent that is scheduled *between* its children
rather than only after them.

This spec depends on:

- §FS-rhei-plan-language for the task hierarchy and the non-leaf eligibility rule
- §FS-rhei-run for the orchestrator loop and the ready set
- §FS-rhei-next for manual claimability
- §FS-rhei-states and §FS-rhei-transitions for the state-machine schema,
  counted loops, and conditions
- §FS-rhei-agents for prompt composition
- §FS-rhei-snapshots for the session continuity a supervisor relies on

The decision behind this shape — a barrier woken after the fact, steering
through the levers that already exist — is recorded in
§DF-subtree-supervision.

## Overview

Without supervision a parent task runs exactly once, after every descendant
is terminal, and sees only what its children left on disk. A review/fix chain
authored as four children runs unattended until the end. With supervision the
parent is a standing participant:

```
supervise (visit 1) → release → 1.1 review → checkpoint → supervise (visit 2) → release → 1.2 fix → …
```

Each visit continues the supervisor's own transcript
(§FS-rhei-snapshots.4.3), reads what reached the checkpoint, and steers the
next step — by writing a *brief* the next child reads, by appending or
cancelling children, or by finishing the parent once the subtree is terminal.
The supervisor is a **barrier over its subtree**: while it is owed a visit,
nothing beneath it is dispatched; while it is running, nothing beneath it
runs.

Supervision deliberately keeps the existing machinery as the levers — plan
edits, artifacts, transitions. It adds one state field, one hold/release rule,
one condition operand, one metadata block, and two prompt sections.

## 1. Declaring a Supervisor

### 1.1. The `supervise` Field

```yaml
states:
  supervise:
    supervise: task          # task | state
    agent: pi
    visits: 20
    snapshot:
      emit:    { name: supervisor, on: always }
      inherit: { name: supervisor, from: self }
    instructions: |
      You supervise Task {task_id} ...
```

| Value | A checkpoint is produced when a descendant… | The supervisor runs… |
|-------|----------------------------------------------|----------------------|
| `task` | enters a terminal state | after every finished descendant |
| `state` | fires any transition, terminal ones included | after every hop of every descendant's own machine |

`supervise` is a property of the *state*, not of the task: a task supervises
while it is in a supervising state and stops when it leaves one. A leaf task
in a supervising state behaves as an ordinary agent state — it has no
descendants, so it is woken once and finishes on its `openDescendants < 1`
edge (§4.1).

Omitting `supervise` leaves the state with today's behavior: a non-leaf task
in it is worked once, after its whole subtree is terminal
(§FS-rhei-plan-language.3).

### 1.2. Validation Rules

- `supervise`, when present, must be `task` or `state`.
- A supervising state must be agent-bearing: it declares `agent`, `target`,
  `model`, or a legacy agent/model selection (§FS-rhei-states.1.2).
  `supervise` on a `final: true`, `gating: true`, `program:`, or `poll:` state
  is a validation error.
- `supervise` combined with `all_targets` or `all_models` is a validation
  error in v1: a supervisor is one continued session, not a fanout.
- A supervising state must declare a self-loop transition
  (`from: <state>, to: <state>`). The self-loop is the *release* edge (§3.1);
  without it the supervisor would run once and never wait for its subtree.
  This mirrors the self-loop rule for polling states (§FS-rhei-states.1.3).
- `rhei validate` warns when no transition from a supervising state uses
  `openDescendants` (§4.1) to reach a terminal state: the supervisor would
  have no way to finish.
- `visits` on a supervising state is allowed and budgets the number of
  supervisor visits; the usual exhaustion rules apply
  (§FS-rhei-transitions.4.3). `rhei validate` warns when a supervising state
  declares neither `visits` nor an exhaustion edge.
- `snapshot.inherit` on a supervising state is allowed and recommended; the
  lineage rules are unchanged (§FS-rhei-snapshots.4.3).

## 2. Checkpoints

### 2.1. Checkpoint Events

A *checkpoint event* is produced on the shared transition path
(§FS-rhei-transition-cmd.3.1) — by `rhei run`, `rhei transition`,
`rhei complete`, or a callback redirect alike — when a transition is applied
to a task that has a supervising ancestor:

- under `supervise: task`, when the applied transition's effective target is
  `final: true`;
- under `supervise: state`, on every applied transition, terminal ones
  included.

A polling state's self-loop attempt (§FS-rhei-states.2) is a retry, not
progress, and never produces a checkpoint. A fanout state's per-invocation
exits are not transitions; the one transition selected once every invocation
has landed is (§FS-rhei-states.3.3).

A transition applied to a descendant while its nearest supervisor is itself
in flight — a cancel the supervisor issues during its own visit (§5.1) — is
not a checkpoint: the supervisor already knows. The shared path recognizes
that visit from the two facts it can see: the supervisor's `**Assignee:**`
claim, and the task id the invocation it is running inside carries
(§FS-rhei-agents.4). A descendant's own worker carries the descendant's id, so
its exits are checkpoints as usual.

### 2.2. Nearest Supervising Ancestor

A checkpoint event is delivered to exactly one task: the **nearest** ancestor
of the transitioning task that is currently in a supervising state. Ancestors
farther up see nothing of it; what they see is the nearer supervisor's own
transitions, per their own `supervise` setting.

A supervisor's **self-loop** exit is never a checkpoint for its ancestors — it
is the supervisor waiting, not the subtree progressing. Every other transition
of a supervisor is an ordinary transition of an ordinary descendant: its
terminal exit is a `task`-level event for the next supervisor up, and its exit
into any other state is a `state`-level one.

### 2.3. Timing

Checkpoints are **post-transition**. The descendant has already moved to its
new state and is held there (§3); the supervisor judges what happened and
steers what comes next. Vetoing or redirecting a transition before it is
applied remains the job of `on_leave` callbacks (§FS-rhei-transitions.3.2);
supervision does not add a second approval path.

## 3. Hold and Release

### 3.1. The Rule

A task `P` in a supervising state is a barrier over its subtree. Its
*supervision phase* is either **held** or **released** (§3.3):

1. **Entry holds.** When `P` enters a supervising state — by any verb, or by
   being authored in it — the phase is `held`: no descendant of `P` is
   dispatched by `rhei run` or claimable by `rhei next`. `P` itself is ready.
2. **The self-loop releases.** When `P` exits the supervising state by its
   self-loop, the phase becomes `released`: descendants of `P` are eligible
   under the ordinary rules — `**Prior:**`, `inputs:`, gating, polling,
   `concurrent`, `--parallel` — exactly as if `P` had no supervising state.
   The visit is over, so the self-loop also drops `P`'s `**Assignee:**`: the
   worker that took the visit no longer holds the ticket, and `P` is claimed
   afresh by whoever answers its next checkpoint (§3.4).
3. **A checkpoint holds again.** When a checkpoint event is delivered to `P`
   (§2), the phase becomes `held` and the event is recorded (§3.3). Nothing
   new beneath `P` is dispatched. Descendants already in flight run to their
   exit, and their exits may deliver further checkpoints, which accumulate.
   Once no descendant of `P` is in flight, `P` is ready.
4. **Leaving releases; finishing is guarded.** When `P` exits the supervising
   state by any edge other than its self-loop, the phase is cleared and its
   descendants follow the ordinary rules. An exit into a `final: true` state
   is subject to the descendants-first guard like any other terminal entry
   (§FS-rhei-transition-cmd.3.1); a machine expresses "finish once the subtree
   is done" with `openDescendants < 1` (§4.1).

Two invariants follow, and they are the point:

- **A supervisor and its descendants are never worked at the same time.** `P`
  is ready only while nothing beneath it is in flight, and nothing beneath it
  is dispatched while `P` is owed a visit or running. This is the property the
  non-leaf model already guarantees for an unsupervised parent
  (§FS-rhei-plan-language.3), extended to a parent that runs many times.
- **A supervisor that changes nothing changes nothing.** Its self-loop
  releases the subtree and the subtree proceeds. Supervision never spins: `P`
  is not ready again until a descendant produces a checkpoint.

Under `--parallel`, rule 3 is a drain: siblings already running finish, no new
ones start, and the supervisor sees every checkpoint they produced in one
visit (§FS-rhei-run.5). A subtree that shares a supervisor therefore
serializes at each checkpoint; `supervise: state` serializes it at every hop
and costs one supervisor invocation per hop, which is the trade an author makes
by choosing it.

### 3.2. Readiness

The ready set of `rhei run` (§FS-rhei-run.3) and the claimability rule of
`rhei next` (§FS-rhei-next.3) gain one rule each, replacing the "every
descendant is terminal" requirement for the tasks it concerns:

- A task in a supervising state is ready when its phase is `held` and no
  descendant of it is in flight. The "every descendant is terminal" condition
  does not apply to it.
- A task with one or more supervising ancestors is ready only when **every**
  supervising ancestor's phase is `released`, in addition to the ordinary
  rules. A supervising ancestor that is `held`, or in flight, holds the whole
  subtree beneath it, nested supervisors included.

A task is *in flight* when a run has spawned it and it has not exited, or when
it carries `**Assignee:**` — the manual worker's claim. Every other task keeps
today's rule unchanged.

### 3.3. Supervision Metadata

The phase and the pending checkpoints are runtime-maintained task metadata in
plan frontmatter, beside `stateVisits` (§FS-rhei-transitions.2.2):

```yaml
metadata:
  tasks:
    1:
      stateVisits:
        supervise: 3
      supervision:
        phase: held                 # held | released
        checkpoints:
          - task: "1.2"
            from: review
            to: fix
            visit: 2
```

- `supervision.phase` is written on the shared transition path: `held` on
  entry into a supervising state and on every delivered checkpoint,
  `released` on the self-loop exit. A task in a supervising state with no
  `supervision` block is `held` — the authored-initial-state case.
- `supervision.checkpoints` accumulates delivered events in delivery order.
  `task` is the rhei-local id of the transitioning descendant, `from` and `to`
  its bare state names, `visit` the `to` state's visit number. The list is
  cleared on the self-loop exit, after the visit that consumed it.
- The block is removed when the task leaves the supervising state by any
  other edge, and by `rhei reset` together with `stateVisits`, which also drops
  a `metadata.tasks.<id>` entry left empty by the two (§FS-rhei-reset).

Nothing here is authored by hand in normal workflows. The block exists so
that a run stopped between a checkpoint and the supervisor's visit resumes
exactly where it was, and so that a manual worker (§3.4) sees the same state
`rhei run` would.

### 3.4. Manual Workers

Supervision is defined on the shared path, so the manual-worker loop sees it
without special casing:

- `rhei next` claims a held supervisor under §3.2 and renders its prompt with
  the checkpoints (§5.1). Claim mode and `--peek` render the same two
  supervision sections `rhei run` composes, from the same renderers and in the
  same order relative to the instructions: `## Checkpoints` for a ticket in a
  supervising state that is owed any, and `## Supervisor Brief` for a ticket a
  supervising ancestor wrote one for (§5.2). A ticket in a supervising state
  also gets `## Supervising This Subtree`, which carries what `rhei run` puts in
  `## Rhei Commands` and `## Result` — sections `rhei next` does not render: the
  brief paths (§5.1), what the barrier means while the visit runs, and how the
  visit ends. Under `--json` each is a field,
  present only when the section is. A plan with no supervising state produces
  the output it always did. It never claims a descendant of a held supervisor;
  such descendants are reported as `Task <id> held by supervisor Task <P>
  (<state>)`, a reason row of its own beside the prerequisite row
  (§FS-rhei-next.3.4). That row ends in the next step, because a held ticket is
  not a stall but someone else's turn: it names the supervisor as the ticket to
  work and gives the command that claims it, or — when a worker already holds
  that visit — names the holder and the `rhei release` that hands it back. The
  supervisor is in no other category the diagnosis reports: its own subtree is
  open, so the "workable" set that feeds them excludes it, and a row that
  stopped at "everything is held" would leave the worker with nowhere to go.
- The worker releases the subtree with the self-loop,
  `rhei transition <P> --from <state> --to <state>`, and finishes it with the
  terminal edge once `openDescendants` is `0`. That self-loop ends the visit
  and with it the worker's claim: `rhei transition` drops `P`'s
  `**Assignee:**` on this one edge, the way a terminal entry does
  (§FS-rhei-transition-cmd.3). A claim that outlived the visit would be read
  as "the supervisor is working right now" — every later descendant exit
  would be taken for the supervisor's own doing and deliver no checkpoint
  (§2.1), and `P` itself would never be scheduled again.
- `rhei list --ready` excludes a held descendant, by the same rule the ready
  set applies (§3.2), so the listing never offers a ticket `rhei run` would
  refuse to schedule; and the run report names the reason on the ticket it
  halted on, so a subtree waiting on its supervisor is not mistaken for a stall
  (§FS-rhei-list, §FS-rhei-run-report). The plain `rhei list` listing carries no
  held reason: it has no readiness-reason column for anything today, and adding
  one is a follow-up alongside the same reason in the TUI and the Flow
  dashboard (§FS-rhei-viz).

## 4. Transition Support

### 4.1. The `openDescendants` Operand

Transition `condition` expressions gain the integer operand
`openDescendants`: the number of descendants of the transitioning task —
child, grandchild, or deeper — whose state is not terminal. A leaf's
`openDescendants` is `0`.

It is evaluated against the plan as **re-read after the subprocess exits**, so
children a supervisor appended during its visit count, and children it
cancelled do not. The operand is available on transitions from any state, not
only supervising ones; it is the operand that lets a machine *select* a
terminal edge for a parent. The descendants-first guard still decides whether
that edge may be taken (§FS-rhei-transition-cmd.3.1) — `openDescendants` is
how a machine agrees with the guard, never a way around it.

The canonical supervisor edges are:

```yaml
transitions:
  - from: supervise
    to: human-review
    description: Supervisor budget exhausted; a human decides
    condition: visitCount >= visits
  - from: supervise
    to: completed
    description: Every child is terminal and the supervisor wrote its result
    condition: openDescendants < 1
  - from: supervise
    to: supervise
    description: Released the subtree; wait for the next checkpoint
```

Transitions are tried in declaration order (§FS-rhei-run.3), so the
exhaustion edge comes first, the terminal edge second, and the unconditional
self-loop last.

### 4.2. Self-Loops on Agent States

A transition whose `from` equals its `to`, on a state that does not declare
`poll:`, is a **loop-back re-entry**: it increments `stateVisits.<state>` like
any other re-entry (§FS-rhei-transitions.4.3), is bounded by `visits` when
declared, emits and inherits snapshots per visit (§FS-rhei-snapshots.10.3),
and re-evaluates the state's `inputs:` on re-entry. The engine selects it on
the same rules as any other edge. On a supervising state it is the release
edge (§3.1); on any other agent state it simply means "run this state again".

The counter is therefore kept for **every** non-poll state the machine
declares a self-loop from, whether or not that state caps itself with
`visits:` — a task that merely enters such a state gets `stateVisits.<state>:
1` — because `visitCount` is what the loop's own exit condition reads. Without
the counter a `condition: visitCount >= N` exit compares against `0` forever
and the loop never ends. Counting does not change how the state is spelled:
`**State:**` takes its `-<n>` suffix only when `visits:` is declared
(§FS-rhei-transitions.2.3), so a machine that adds no budget sees no change in
its plan files.

`rhei validate` warns when a non-poll state with a self-loop declares neither
`visits:` nor a transition bounded by `visitCount`: nothing terminates that
loop (§FS-rhei-states.1.3).

This generalizes the self-loop, previously specified only for polling states
(§FS-rhei-states.2). The poll interpretation — release the slot, retry after
`interval` — is unchanged and applies only when the state declares `poll:`.

## 5. Prompt Composition

### 5.1. The Supervisor's Prompt

An invocation of a supervising state renders two sections after
`## Task Content` (§FS-rhei-agents.3):

```
## Child Tasks

- Task plan.1.1: Review parser [completed]
- Task plan.1.2: Fix findings [fix]
- Task plan.1.3: Re-review [review]

## Checkpoints

These are the descendants that moved since your last visit, in order. Each
carries what that step left behind.

### Task plan.1.2: Fix findings — review → fix (visit 2)

{for a terminal `to`: the descendant's result, `runtime/results/<id>.md`;
 otherwise: every declared, existing, non-empty `outputs:` artifact of the
 `from` state, each under its artifact name}
```

`## Child Tasks` is the map and is rendered on every visit. `## Checkpoints`
renders the recorded `supervision.checkpoints` (§3.3) and is omitted on a
visit with none — the first visit, where the supervisor's job is to brief the
first step.

Both sections spell a descendant the one way the supervisor can act on it: the
**qualified** id, the same one `rhei transition` accepts. `supervision.checkpoints`
stores the rhei-local id (§3.3), and the renderer resolves it to exactly one
descendant — the recorded id under the supervisor's own qualification — never
to a deeper node whose id happens to end the same way.

A non-leaf task in a state that is *not* supervising renders `## Child Tasks`
as today and, new, `## Child Task Results` — the result of every terminal
child, in plan order — so an unsupervised parent integrating its subtree also
sees what the subtree produced.

The `## Rhei Commands` section of a supervisor's prompt names the lever the
supervisor steers with before the ones it destroys with: one sentence giving
both brief paths (§5.2) with the execution root resolved to an absolute path,
because the supervisor's working directory is not something the prompt can
promise. It additionally states
that the agent **may** run `rhei transition` against *held descendants* — to
cancel a step the checkpoint made unnecessary, typically, passing
`--result "<why>"` because a cancelled ticket still has to say why (§6) — and
may append descendants under its own task in its task file, as any agent
editing its own file may. It still must not transition its own task: the orchestrator owns
that edge. A transition the supervisor applies to a descendant is an external
plan change the orchestrator respects on re-read (§FS-rhei-agents.5.2).

### 5.2. The Brief

A supervisor steers a descendant by writing a **brief**, a Markdown file at
one of two reserved paths under the execution root:

- `runtime/supervise/<task-id>.md` — read by every state of that descendant;
- `runtime/supervise/<task-id>/<state>.md` — read by one state only.

When the descendant is invoked, each brief that exists and is non-empty is
rendered, task-level first, under:

```
## Supervisor Brief

These are directions from the supervising Task {supervisor-id}. Follow them
within this state's instructions and artifact contract: a brief may narrow or
direct the work, but it cannot waive a required output or choose the
transition.
```

The brief is how a `review → fix` chain carries the review's verdict into the
fix and the supervisor's judgement into both. It is an ordinary artifact: a
machine may additionally declare it as an `inputs:` entry, optional or
required (§FS-rhei-states.3.1), to gate a state on its presence, but no
declaration is needed for it to be rendered. Briefs are not cleared by the
engine; a supervisor that wants a fresh one overwrites it.

## 6. Interaction With Other Features

- **Snapshots.** A supervising state should `emit` and `inherit` one snapshot
  name `from: self` so each visit continues the previous one
  (§FS-rhei-snapshots.4.3). A descendant may branch from the supervisor's
  transcript with `from: ancestor` (§FS-rhei-snapshots.6) when a step should
  start inside the supervisor's context. Agents without session support run
  each visit cold (§FS-rhei-snapshots.9.3); supervision still works, carried
  by the checkpoints and the briefs.
- **Counted loops.** Each supervisor visit is a visit of the supervising
  state. `visits` budgets them; the exhaustion edge is the safety valve for a
  subtree that never converges.
- **Gating descendants.** A descendant at a human gate is not in flight, so it
  does not keep the subtree from being quiescent. Under `supervise: state` its
  entry into the gate is a checkpoint — the supervisor can cancel the step or
  leave it to the human; under `supervise: task` it is not, and the subtree
  waits on the human as it would unsupervised.
- **Polling descendants.** Poll self-loops are not checkpoints (§2.1); a
  polling descendant between attempts is not in flight.
- **Appending and cancelling.** The supervisor appends descendants by editing
  its task file and cancels them with `rhei transition`, both during its
  visit; the orchestrator re-reads the plan when the visit ends, so
  `openDescendants` (§4.1) and the released subtree reflect the edits. A cancel
  does not have to satisfy the cancelled step's own `outputs:` — cancellation
  abandons the work, so that contract is moot (§FS-rhei-transitions.4.5) — but
  it does have to say why: the terminal-result obligation stands, so every
  cancel carries `--result "<why>"`. The waiver keys on the reserved state name
  (§FS-rhei-states.1.4): a machine whose abandon state is spelled anything else
  gets the ordinary outputs check, and the refusal says which name skips it.
- **Reset.** `rhei reset` on a supervisor clears its `supervision` block;
  resetting a descendant does not touch the supervisor's phase.
- **Fanout.** Not supported on a supervising state in v1 (§1.2).
  Descendants may fan out freely; their merged transition is the checkpoint.

## 7. Example

A pre-authored review/fix chain, supervised after every task:

```markdown
### Task 1: Harden the parser
**State:** supervise

Goal and acceptance criteria for the whole change.

#### Task 1.1: Review parser
**State:** review
#### Task 1.2: Fix findings
**State:** fix
**Prior:** Task 1.1
#### Task 1.3: Re-review
**State:** review
**Prior:** Task 1.2
#### Task 1.4: Fix remaining
**State:** fix
**Prior:** Task 1.3
```

```yaml
states:
  supervise:
    supervise: task
    agent: pi
    visits: 12
    snapshot:
      emit:    { name: supervisor, on: always }
      inherit: { name: supervisor, from: self }
    instructions: |
      You supervise Task {task_id}. Judge every checkpoint below. Brief the
      next step at runtime/supervise/<child-id>.md, append a child if the plan
      needs one, cancel a child the results made unnecessary. When every child
      is terminal, write your result and finish.
  review:
    agent: claude-code
    outputs:
      - name: findings
        path: runtime/review/{task_id}.md
    instructions: Review as briefed; write findings to {output.findings.path}.
  fix:
    agent: claude-code
    instructions: Apply exactly the fixes the brief asks for.
  human-review:
    gating: true
  completed:
    final: true
  cancelled:
    final: true

transitions:
  - { from: supervise, to: human-review, description: Budget exhausted, condition: visitCount >= visits }
  - { from: supervise, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervise, to: supervise, description: Released; wait for the next checkpoint }
  - { from: review, to: completed, description: Findings written }
  - { from: fix, to: completed, description: Fixes applied }
  - { from: "*", to: cancelled, description: Dropped }
```

The run then proceeds:

```
pass 1  Task 1 held, nothing in flight  → supervise v1: briefs 1.1 → self-loop (released)
pass 2  1.1 ready → review runs → completed: checkpoint → Task 1 held
pass 3  supervise v2 (inherits v1): reads 1.1's result, briefs 1.2 → released
pass 4  1.2 fix runs → completed: checkpoint
pass 5  supervise v3: if 1.1 found nothing, cancels 1.3 and 1.4 → released
        …
last    openDescendants = 0 → supervise vN writes its result → completed
```

Change `supervise: task` to `supervise: state` and a child that runs its own
`review → fix` loop would hand control back at every hop as well.

## Related Specifications

- [Plan Language — Semantic Constraints](rhei-plan-language.spec.md#3-semantic-constraints)
- [Run — Execution Loop](rhei-run.spec.md#3-execution-loop)
- [Next — Default Behavior](rhei-next.spec.md#3-default-behavior-claim-mode)
- [States — Per-state fields](rhei-states.spec.md#12-per-state-fields)
- [Transitions — Counted Loops](rhei-transitions.spec.md#43-counted-loops)
- [Agents — Prompt Composition](rhei-agents.spec.md#3-prompt-composition)
- [Snapshots — Lineage Resolution](rhei-snapshots.spec.md#43-lineage-resolution)
