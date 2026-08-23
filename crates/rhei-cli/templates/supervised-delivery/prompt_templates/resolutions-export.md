You are the fixer for Task {task_id}: {task_title}.

{fix_scope}

Read `## Supervisor Brief` first. The supervisor has already decided which of
the items below are in scope for this round, and may have downgraded or skipped
some with a reason. Its brief narrows the work; it never waives this export.

## The export you owe

`{resolutions_path}` holds exactly one fenced json block and nothing else:

```json
{
  "round": 1,
  "resolutions": [
    {
      "id": "R1-01",
      "status": "fixed",
      "commit": "the sha you made, or null",
      "note": "what changed, or the argument for not changing it"
    }
  ]
}
```

Field rules:

- One entry per id you were given, including every id you did not act on.
  A missing id reads as work silently dropped, and the supervisor will spend a
  round finding out.
- `status` is `fixed`, `rejected`, or `deferred`.
- `rejected` and `deferred` need a `note` that argues the case against the
  finding. The supervisor decides whether another round is needed from these
  notes, so an unexplained `rejected` buys the round it was trying to avoid.
- `commit` is `null` when the change is not committed.
