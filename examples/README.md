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

- `review-fix-visits/`
  Valid example directory using:
  - `index.rhei.md` plus `tasks/`
  - `states.yaml`
  - counted `review` and `fix` states with two total passes each
  - one review artifact file and one fix artifact updated across the loop

- `states-with-spaces.yaml`
  Companion states file for `escaped-state-values.rhei.md`.

## Verification commands

Each example is registered with the `xtask` build helper, which selects the
correct state machine and runtime setup per example:

```bash
cargo xtask examples list                      # show all examples
cargo xtask examples validate <name>           # validate one example
cargo xtask examples validate --all            # validate every example
cargo xtask examples run living-review-loop    # run a runnable example in a tmp copy
```

Direct CLI invocations still work if you need a one-off — for example, rendering
an example as JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

## Notes on current behavior

These examples are aligned to the current repository behavior, including the following implementation details:

- task-level descriptive paragraphs are accepted by the parser but are not currently preserved in the AST or render outputs
- subtask numbering is validated only for numeric parent task identifiers
- named parent tasks with subtasks may produce a validation warning rather than an error
- state values with spaces must be written using backslash escapes and validated against the selected states file
