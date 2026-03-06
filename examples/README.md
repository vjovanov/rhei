# Example Plans

This directory contains example inputs for the current markdown plan compiler implementation.

## Files

- `release-automation.saga.md`
  Valid example using:
  - saga overview and requirements sections before `## Tasks`
  - numeric and named task identifiers
  - `**Prior:**` dependencies across numeric and named tasks
  - fenced code block content inside a subtask
  - default states from `docs/state-machine.yaml`

- `human-review-loop.saga.md`
  Valid example using:
  - numeric task identifiers
  - default workflow states including `human-review` and `agent-review`
  - normal subtask content bodies
  - dependency chaining across multiple tasks

- `escaped-state-values.saga.md`
  Valid example using:
  - escaped spaces in `**State:**` values such as `in\ review`
  - a companion custom state machine file because those states are not present in the default machine

- `state-machine-with-spaces.yaml`
  Companion state machine for `escaped-state-values.saga.md`.

## Verification commands

Validate the examples with the CLI:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.saga.md
cargo run -p rhei-cli -- validate examples/human-review-loop.saga.md
cargo run -p rhei-cli -- --state-machine examples/state-machine-with-spaces.yaml validate examples/escaped-state-values.saga.md
```

Render an example as JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.saga.md --format json --pretty
```

## Notes on current behavior

These examples are aligned to the current repository behavior, including the following implementation details:

- task-level descriptive paragraphs are accepted by the parser but are not currently preserved in the AST or render outputs
- subtask numbering is validated only for numeric parent task identifiers
- named parent tasks with subtasks may produce a validation warning rather than an error
- state values with spaces must be written using backslash escapes and validated against the selected state machine
