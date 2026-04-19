# Bash Agent Team Workflow

This directory contains a checked-in workspace fixture for CLI end-to-end tests.

It demonstrates:

- a directory workspace (`index.rhei.md` plus `tasks/`)
- a custom state machine for team handoffs
- bash-based `cli:` callbacks
- a mock kickoff command on the first transition
- full transition execution with runtime logs and artifacts

## Test Usage

The e2e tests copy this directory into the repository `scratchpad/` folder
before invoking `rhei run` and `rhei reset`. That keeps generated task updates,
logs, and artifacts in the shared gitignored area instead of beside the
checked-in fixture. Keep the checked-in task files here in their initial
`pending` state so the fixture stays reusable.

The checked-in paths under this fixture are:

- `tasks/*.md` for the initial workspace task files
- `team-states.yaml` for the bash callback state machine
- `workflow.sh` for the callback implementation

## Current CLI Callback Contract

In the current implementation, `cli:` callbacks run as shell commands with
environment variables:

- `RHEI_TASK_ID`
- `RHEI_FROM_STATE`
- `RHEI_TO_STATE`
- `RHEI_PLAN_PATH`

This fixture is written against that behavior so it works with the current CLI.
