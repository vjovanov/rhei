# DF-subtree-supervision: A supervisor is a barrier over its subtree, woken after the fact

## Status

accepted

## Context

The non-leaf task model made a parent a task in its own right, worked once
after every descendant is terminal and never concurrently with one of them
(§FS-rhei-plan-language.3). That gives a parent exactly one chance to look at
its subtree: at the end, with nothing left to steer. A review/fix chain
authored as four children runs unattended, and the parent's hard-won context —
why the work was decomposed this way, what "good" looks like — is not in the
room while the children run.

The ask was a parent that acts as a *supervisor*: woken after each subtask,
its own session continued, able to judge what just happened and steer what
comes next. Four shapes were weighed. §FS-rhei-supervision

1. **Incremental decomposition, no engine change.** The parent holds the
   chain as a queue in its body and appends one child per visit; today's rule
   already schedules it between children it dispatches itself. Works now, but
   the plan does not show the chain as tasks until they are dispatched, so
   `rhei list` and the dashboard cannot see what is coming.
2. **Artifact gates on the children.** Pre-authored children whose states
   declare an `inputs:` brief the supervisor writes, plus a readiness rule
   that wakes the parent when its subtree is "quiescent". Works for
   after-every-task, but after-every-*state* needs a brief per child-state
   declared on every state of the child's machine, which couples a reusable
   machine to being supervised.
3. **A transition callback that resumes the parent's session.** No engine
   change, but the supervisor runs off the books — no cost accounting, no
   logs, outside the supervised process group (§DA-supervised-process-groups)
   — and can only approve or reject the one transition it fires on.
4. **A pre-transition veto**, where the supervisor decides the child's edge.
   Duplicates what `on_leave` callbacks already are
   (§FS-rhei-transitions.3.2) and would make the supervisor a second
   transition authority beside `rhei run`.

## Decision

Supervision is an engine-owned **hold/release barrier** declared on a state
with `supervise: task | state` (§FS-rhei-supervision.1). The value picks the
checkpoint set; the hold moves into the engine so the children's states stay
plain and reusable unsupervised.

- A supervisor and its descendants are never worked at the same time. Entry
  holds the subtree; the supervisor's self-loop releases it; a checkpoint
  holds it again and the supervisor is ready once nothing beneath it is in
  flight (§FS-rhei-supervision.3.1). This is the non-leaf model's existing
  guarantee, extended to a parent that runs many times.
- Checkpoints are **post-transition** and are delivered to the **nearest**
  supervising ancestor only (§FS-rhei-supervision.2). The supervisor judges
  what happened; vetoing stays with callbacks.
- The supervisor steers with the levers that already exist: a *brief* the
  next step reads, plan edits that append children, `rhei transition` that
  cancels held ones, and its own terminal edge selected by the new
  `openDescendants` operand (§FS-rhei-supervision.4.1,
  §FS-rhei-supervision.5). The descendants-first guard still decides whether
  that edge may be taken (§FS-rhei-transition-cmd.3.1).
- The phase and the pending checkpoints live in plan frontmatter beside
  `stateVisits` (§FS-rhei-supervision.3.3), written on the shared transition
  path, so a stopped run resumes where it was and a manual worker sees the
  same state the orchestrator would.
- A supervisor that changes nothing changes nothing: its self-loop releases
  the subtree, and it is not ready again until a descendant produces a
  checkpoint. Supervision never spins.

## Consequences

- After-every-task and after-every-state supervision both fall out of one
  rule; `state` is the expensive setting — it serializes the subtree at every
  hop and spends one supervisor invocation per hop — and the spec says so.
- Pre-authored chains stay visible as tasks from the start, so every surface
  that explains readiness needs a "held by supervisor" reason row
  (§FS-rhei-supervision.3.4) or a held subtree reads as a stall.
- Self-loops become a general loop-back re-entry on agent states
  (§FS-rhei-supervision.4.2) rather than a polling-only construct, and
  `condition:` expressions can see the subtree through `openDescendants`; the
  sentence in §FS-rhei-transition-cmd.3.1 that said machines cannot see
  children is revised to say they can select but never permit.
- Real context continuity depends on the agent's snapshot session support;
  with the built-in `claude-code` profile still unsupported
  (§FS-rhei-snapshots.9.3) a supervisor runs each visit cold, carried by
  checkpoints and briefs. That adapter is tracked on the roadmap
  independently (§RM-rhei-roadmap).
