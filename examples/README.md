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

- `pm-onboarding-experiment.rhei.md`
  Valid example using:
  - a product-manager-oriented plan with context sections and success metrics
  - numeric task identifiers and subtasks
  - a linear dependency chain from planning through launch recommendation

- `escaped-state-values.rhei.md`
  Valid example using:
  - escaped spaces in `**State:**` values such as `in\ review`
  - a companion custom states file because those states are not present in the default set

- `claude-code/`
  Valid example directory using:
  - `plan.rhei.md`
  - `states.yaml`
  - `**States:** claude-code-simple`
  - a Claude Code least-privilege workflow with only simple states

- `living-review-loop/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - `team-states.yaml`
  - orchestrator callbacks that append new workspace task files during `rhei run`
  - a shared findings artifact followed by verification and selective fix tasks

- `states-with-spaces.yaml`
  Companion states file for `escaped-state-values.rhei.md`.

## Verification commands

Validate the examples with the CLI:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.rhei.md
cargo run -p rhei-cli -- validate examples/human-review-loop.rhei.md
cargo run -p rhei-cli -- validate examples/pm-onboarding-experiment.rhei.md
cargo run -p rhei-cli -- --state-machine examples/states-with-spaces.yaml validate examples/escaped-state-values.rhei.md
cargo run -p rhei-cli -- --state-machine examples/claude-code/states.yaml validate examples/claude-code/plan.rhei.md
cargo run -p rhei-cli -- --state-machine examples/living-review-loop/team-states.yaml validate examples/living-review-loop
```

Render an example as JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

Run the living workspace example end to end in a disposable copy:

```bash
tmp_dir="$(mktemp -d)"
cp -R examples/living-review-loop "$tmp_dir/living-review-loop"
cargo run -p rhei-cli -- --state-machine "$tmp_dir/living-review-loop/team-states.yaml" run "$tmp_dir/living-review-loop"
```

## Notes on current behavior

These examples are aligned to the current repository behavior, including the following implementation details:

- task-level descriptive paragraphs are accepted by the parser but are not currently preserved in the AST or render outputs
- subtask numbering is validated only for numeric parent task identifiers
- named parent tasks with subtasks may produce a validation warning rather than an error
- state values with spaces must be written using backslash escapes and validated against the selected states file
