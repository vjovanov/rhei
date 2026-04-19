# Bash Agent Team Workflow

This example is a runnable directory workspace for `rhei run`.

It demonstrates:

- a directory workspace (`index.rhei.md` plus `tasks/`)
- a custom state machine for team handoffs
- bash-based `cli:` callbacks
- a mock kickoff command on the first transition

## Run It

Run these commands from the repository root:

```bash
cargo run -p rhei-cli -- --state-machine examples/bash-agent-team/team-states.yaml validate examples/bash-agent-team
cargo run -p rhei-cli -- --state-machine examples/bash-agent-team/team-states.yaml run examples/bash-agent-team
```

After `run` completes, inspect:

- `examples/bash-agent-team/tasks/*.md` for the final task states
- `examples/bash-agent-team/runtime/logs/` for the transition log
- `examples/bash-agent-team/runtime/artifacts/` for per-task outputs

## Current CLI Callback Contract

In the current implementation, `cli:` callbacks run as shell commands with
environment variables:

- `RHEI_TASK_ID`
- `RHEI_FROM_STATE`
- `RHEI_TO_STATE`
- `RHEI_PLAN_PATH`

This example is written against that behavior so it works with the current CLI.

