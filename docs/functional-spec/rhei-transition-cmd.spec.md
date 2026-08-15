# FS-rhei-transition-cmd: `rhei transition`

Atomically advance a task's state using compare-and-swap semantics. `rhei transition` is the coordination primitive for manual workers and concurrent agents: only the caller whose expected `--from` matches the task's actual current state wins the race, and every transition is validated against the active state machine before any write.

## 1. Usage

```bash
rhei transition <TICKET_ID> --from <STATE> --to <STATE>
rhei transition <RHEI_PLAN> --task <TASK_ID> --from <STATE> --to <STATE>
```

The positional slot is a *ticket or plan*, on the shared rule every
single-ticket command follows (§FS-rhei-usage.2): an argument naming an
existing path is the plan, an id-shaped argument naming no path is the ticket.
The ticket must be named one way or the other.

## 2. Options

| Flag             | Required | Default | Description                                                                 |
|------------------|----------|---------|-----------------------------------------------------------------------------|
| `--task <ID>`    | Unless named positionally | | Ticket identifier: project-qualified (`auth.1`) or rhei-local (`1`). See §2.1. |
| `--from <STATE>` | Yes      |         | Expected current state (compare-and-swap guard)                             |
| `--to <STATE>`   | Yes      |         | Target state                                                                |
| `--no-callbacks` | No       | false   | Skip execution of `on_leave` / `on_enter` callbacks registered on the edge  |

State values passed to `--from` and `--to` follow the state-value rendering rules in the [main spec](rhei-plan-language.spec.md#32-state-validity): bare for names that match `IDENTIFIER`, backtick-wrapped otherwise.

### 2.1. Ticket Targets

The ticket target — positional or `--task` — accepts either the
project-qualified ticket id (`auth.1`, numbers or names in either segment) or a
rhei-local shorthand (`1`). A shorthand resolves
only when exactly one rhei in the project contains that ticket; when more than
one does, the error names the qualified candidates. Output, the result file,
and the ledger entry always use the qualified id regardless of how the target
was written (§FS-rhei-panta.6).

`rhei transition` takes no `--rhei` flag: the explicit ticket target already
names the scope. The rewrite is routed to the file of the rhei that owns the
ticket, under that rhei's own rhei-local heading (§FS-rhei-panta.6.1).

## 3. Behavior

1. Load the state machine and plan (single-file or directory workspace). Validate.
2. Locate the task by id. Fail if it does not exist.
3. Acquire a file lock on the plan file (single-file plan) or on the task file that contains the task (directory workspace).
4. Re-read the task's current state under the lock. If it does not equal `--from`, fail with a compare-and-swap conflict error and print the actual current state.
5. Validate that a declared transition exists from `--from` to `--to` in the active state machine. Reject if the edge is unlisted.
6. Apply the descendants-first guard (§3.1). Reject before any callback runs
   when `--to` is a `final: true` state and the task still has a non-terminal
   descendant. The guard runs after step 5, so an edge the machine never
   declared is reported as an unlisted edge rather than as an open subtree: a
   user is not sent to finish descendants for a move that was never available.
7. Execute the `on_leave` callback on the source state, if any, unless `--no-callbacks` is set.
8. Verify that every required `outputs:` artifact declared on the source state exists (see [Plan Language Specification — State Artifact Contracts](rhei-plan-language.spec.md#310-state-artifact-contracts)). Missing outputs abort the transition before the state write.
9. Resolve the target state's `inputs:` artifacts. Missing required inputs abort the transition before the state write; optional inputs are resolved but do not block entry.
10. Rewrite the task's `**State:**` line to the new state value (with counted-visit suffix when applicable).
11. Execute the `on_enter` callback on the target state, if any, unless `--no-callbacks` is set.
12. Write the task file atomically (temp file + rename) and release the lock.
13. Append one state-transition entry to `runtime/state-transitions.log` as
    `<task-id> <from>@<to>`, creating the `runtime/` directory if needed. The
    file is the central, deterministic audit trail for all task state changes.

`rhei transition` does not add, remove, or modify the `**Assignee:**` line. Assignment and unassignment are owned by `rhei next` and `rhei complete` respectively.

`rhei transition` deliberately does **not** check `**Prior:**` dependencies.
It is the explicit human-initiated primitive, so it is the escape hatch for
the moves the scheduling commands refuse: leaving a gating state
(§FS-rhei-complete.4), and advancing a ticket ahead of an unsatisfied prior.
`rhei next`, `rhei run`, and `rhei complete` all enforce readiness; a caller
that reaches for `transition` is stating the out-of-order move is intended.
Because the resulting plan then contradicts its own declared dependencies,
`rhei validate` reports it as a warning (§FS-rhei-validate.4) rather than
letting it pass unremarked.

Counted-visit accounting: if the target state declares a `visits` budget and `--to` is a loop-back re-entry, the runtime increments `metadata.tasks.<id>.stateVisits.<target>` and renders the new visit number in `**State:**` using the `-<n>` suffix. See [Transitions Specification — Counted Loops](rhei-transitions.spec.md#43-counted-loops).

### 3.1. Descendants-First on Terminal Entry

A transition into a `final: true` state is rejected while the task has any
non-terminal descendant — child, grandchild, or deeper. The error names the
target state and every open descendant as `Task <id> (<state>)` — the same
shape `rhei next` (§FS-rhei-next.3.4) and the run report (§FS-rhei-run-report.3.1)
print, so a user moving between the three verbs reads one format. Its guidance
names the commands that reveal and claim the open work rather than only
restating the rule (§FS-rhei-errors.2).

This guard lives on the **shared transition path**, beside compare-and-swap,
`outputs:`/`inputs:` enforcement, and callbacks. It therefore applies
identically to every verb that can move a task into a terminal state:
`rhei transition`, `rhei complete` (§FS-rhei-complete.4), `rhei run`'s
orchestrator-owned auto-advance (§FS-rhei-run.3), and a callback that redirects
an edge with `nextState` (§FS-rhei-transitions.3.2) — a redirect is re-checked
against the effective target, so it cannot smuggle a terminal entry past the
guard. No command holds a private copy of the rule, and no state machine can
opt out of it: transition `condition:` expressions see only visit and exit-code
variables (§FS-rhei-states.2.3), so a machine author has no way to gate a
parent's terminal edge on its children. The engine must.

The guard is deliberately **not** symmetric with `**Prior:**` readiness, which
`rhei transition` skips as the human escape hatch (§3). The line between the
two is the one `rhei validate` already draws: a terminal parent with an open
descendant is an **error**, an out-of-order prior is a **warning**
(§FS-rhei-validate.4). `rhei transition` may deliberately produce a warning; it
must never be able to produce an error. A parent that genuinely must finish
ahead of its subtree is finished by finishing or cancelling the subtree first —
`cancelled` is terminal, so an abandoned child satisfies the guard.

`rhei next` is unaffected by this guard because claiming does not advance state
(§FS-rhei-next.3); it applies the eligibility rule instead
(§FS-rhei-plan-language.3).

## 4. Compare-and-Swap Conflicts

Two agents that race on the same task both specify the same `--from`. The first call to acquire the lock rewrites the state. The second call re-reads under the lock, sees the actual state no longer matches `--from`, and fails non-zero with:

```text
Error: Task <ID> is in state '<actual>', not '<from>'.
       Another transition may have preceded this call.
```

Losers are expected to re-read the plan and either re-select with `rhei next` or retry against the new state.

## 5. Output

On success:

```text
Task <ID> transitioned: '<from>' -> '<to>'
```

With `--no-callbacks`:

```text
Task <ID> transitioned: '<from>' -> '<to>' (callbacks skipped)
```

## Relationship to Other Commands

| Command            | What it does                                                                    |
|--------------------|---------------------------------------------------------------------------------|
| `rhei next`        | Claims the next ready task (assigns without transitioning), prints instructions |
| `rhei next --peek` | Read-only: prints the next claimable task without claiming it                   |
| `rhei transition`  | Atomically changes a task's state; appends entry to result file                 |
| `rhei complete`    | Transitions to terminal, appends result entry, links file, unassigns            |
| `rhei reset`       | Returns each task to its resolved profile's `initial` state, removes `runtime/`; narrowed with `--rhei <id>` it removes only the in-scope tickets' keyed output (§FS-rhei-reset.2.1) |

The typical agent loop is: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release).

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — state-value grammar and validation rules
- [States Specification](rhei-states.spec.md) — state machine format
- [Transitions Specification](rhei-transitions.spec.md) — transition YAML schema, callbacks, and counted-loop accounting
- [Callbacks Specification](rhei-callbacks.spec.md) — `on_leave` / `on_enter` callback examples
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
