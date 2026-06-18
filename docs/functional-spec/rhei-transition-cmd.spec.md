# FS-rhei-transition-cmd: `rhei transition`

Atomically advance a task's state using compare-and-swap semantics. `rhei transition` is the coordination primitive for manual workers and concurrent agents: only the caller whose expected `--from` matches the task's actual current state wins the race, and every transition is validated against the active state machine before any write.

## 1. Usage

```bash
rhei transition <RHEI_PLAN> --task <TASK_ID> --from <STATE> --to <STATE>
```

## 2. Options

| Flag             | Required | Default | Description                                                                 |
|------------------|----------|---------|-----------------------------------------------------------------------------|
| `--task <ID>`    | Yes      |         | Task identifier (number or name)                                            |
| `--from <STATE>` | Yes      |         | Expected current state (compare-and-swap guard)                             |
| `--to <STATE>`   | Yes      |         | Target state                                                                |
| `--no-callbacks` | No       | false   | Skip execution of `on_leave` / `on_enter` callbacks registered on the edge  |

State values passed to `--from` and `--to` follow the state-value rendering rules in the [main spec](rhei-plan-language.spec.md#32-state-validity): bare for names that match `IDENTIFIER`, backtick-wrapped otherwise.

## 3. Behavior

1. Load the state machine and plan (single-file or directory workspace). Validate.
2. Locate the task by id. Fail if it does not exist.
3. Acquire a file lock on the plan file (single-file plan) or on the task file that contains the task (directory workspace).
4. Re-read the task's current state under the lock. If it does not equal `--from`, fail with a compare-and-swap conflict error and print the actual current state.
5. Validate that a declared transition exists from `--from` to `--to` in the active state machine. Reject if the edge is unlisted.
6. Execute the `on_leave` callback on the source state, if any, unless `--no-callbacks` is set.
7. Verify that every required `outputs:` artifact declared on the source state exists (see [Plan Language Specification — State Artifact Contracts](rhei-plan-language.spec.md#310-state-artifact-contracts)). Missing outputs abort the transition before the state write.
8. Resolve the target state's `inputs:` artifacts. Missing required inputs abort the transition before the state write; optional inputs are resolved but do not block entry.
9. Rewrite the task's `**State:**` line to the new state value (with counted-visit suffix when applicable).
10. Execute the `on_enter` callback on the target state, if any, unless `--no-callbacks` is set.
11. Write the task file atomically (temp file + rename) and release the lock.
12. Append one state-transition entry to `runtime/state-transitions.log` as
    `<task-id> <from>@<to>`, creating the `runtime/` directory if needed. The
    file is the central, deterministic audit trail for all task state changes.

`rhei transition` does not add, remove, or modify the `**Assignee:**` line. Assignment and unassignment are owned by `rhei next` and `rhei complete` respectively.

Counted-visit accounting: if the target state declares a `visits` budget and `--to` is a loop-back re-entry, the runtime increments `metadata.tasks.<id>.stateVisits.<target>` and renders the new visit number in `**State:**` using the `-<n>` suffix. See [Transitions Specification — Counted Loops](rhei-transitions.spec.md#43-counted-loops).

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
| `rhei reset`       | Returns each task to its resolved profile's `initial` state, removes `runtime/` |

The typical agent loop is: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release).

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — state-value grammar and validation rules
- [States Specification](rhei-states.spec.md) — state machine format
- [Transitions Specification](rhei-transitions.spec.md) — transition YAML schema, callbacks, and counted-loop accounting
- [Callbacks Specification](rhei-callbacks.spec.md) — `on_leave` / `on_enter` callback examples
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
