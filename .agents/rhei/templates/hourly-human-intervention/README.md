# Hourly Human Intervention Template

Instantiate this from the coordination workspace root:

```sh
rhei instantiate hourly-human-intervention --output .agents/scratchpad/hourly-human-intervention-$(date -u +%Y%m%dT%H%M%SZ)
```

The instantiate output prints the generated file tree, the task tree, the last
few tasks, and the current stop point. For a fresh workspace it stops before
execution because `--execute` was not passed.

Then run the instantiated workspace:

```sh
rhei run .agents/scratchpad/hourly-human-intervention-<timestamp> --parallel 1
```

The template creates two initial tasks:

- fetch and classify open `human-intervention` issues
- fetch and classify open `human-intervention` pull requests

Each classification creates child tasks that run through deep analysis, route
selection, and the matching fix path:

- CI failure triage, restart/rerun, and completion or fix routing
- GitHub human handoff
- Forge fix and two review passes
- GraalVM proposal, human review, fix, and two review passes

Keep the default serial run unless each mutating task has been moved to its own
worktree. The template edits shared local checkouts.

The `states.yaml` file is a template and contains input placeholders. To inspect
the concrete state machine, instantiate it first and run `rhei states` against
the generated `states.yaml`.
