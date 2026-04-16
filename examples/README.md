# Example Plans

This directory contains example inputs for the current markdown plan compiler implementation.

## Files

- `release-automation.rhei.md`
  Valid example using:
  - rhei overview and requirements sections before `## Tasks`
  - numeric and named task identifiers
  - `**Prior:**` dependencies across numeric and named tasks
  - fenced code block content inside a subtask
  - default states from `docs/states.yaml`

- `human-review-loop.rhei.md`
  Valid example using:
  - numeric task identifiers
  - default workflow states including `human-review` and `agent-review`
  - normal subtask content bodies
  - dependency chaining across multiple tasks

- `escaped-state-values.rhei.md`
  Valid example using:
  - escaped spaces in `**State:**` values such as `in\ review`
  - a companion custom states file because those states are not present in the default set

- `states-with-spaces.yaml`
  Companion states file for `escaped-state-values.rhei.md`.

## Verification commands

Validate the examples with the CLI:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.rhei.md
cargo run -p rhei-cli -- validate examples/human-review-loop.rhei.md
cargo run -p rhei-cli -- --state-machine examples/states-with-spaces.yaml validate examples/escaped-state-values.rhei.md
```

Render an example as JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

## Notes on current behavior

These examples are aligned to the current repository behavior, including the following implementation details:

- task-level descriptive paragraphs are accepted by the parser but are not currently preserved in the AST or render outputs
- subtask numbering is validated only for numeric parent task identifiers
- named parent tasks with subtasks may produce a validation warning rather than an error
- state values with spaces must be written using backslash escapes and validated against the selected states file
