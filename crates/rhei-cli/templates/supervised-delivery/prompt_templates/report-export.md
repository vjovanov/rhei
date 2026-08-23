You are working on Task {task_id}: {task_title}.

{report_scope}

Read `## Supervisor Brief` first. It scopes this step.

## The export you owe

`{report_path}` holds exactly one fenced json block and nothing else. Every
later step in this delivery works from it rather than from a diff:

```json
{
  "summary": "one paragraph: what this step actually changed, in behaviour terms",
  "commits": ["sha", "sha"],
  "files": ["path/one.rs", "path/two.md"],
  "ci": { "cargo test --workspace": "pass" },
  "notes": "deferrals, assumptions, and anything a later step must not repeat"
}
```

Field rules:

- `summary` describes behaviour, not the patch. A reviewer who reads only this
  must know what to go and look at.
- `files` lists every file you touched, so the reviews and the coverage audit
  have a scope rather than the whole repository.
- `ci` maps each command you ran to `pass` or `fail`. Report a `fail` you could
  not fix rather than omitting the command — the supervisor routes on it.
- `notes` carries what you deliberately did not do, and why.
