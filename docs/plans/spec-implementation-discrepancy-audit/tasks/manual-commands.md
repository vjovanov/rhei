### Task manual-commands: Audit manual worker and inspection commands
**State:** scope-spec

Audit the command contracts used by humans and manual agents outside the full
`rhei run` orchestrator.

Spec scope:

- `docs/specs/rhei-next.spec.md`
- `docs/specs/rhei-transition-cmd.spec.md`
- `docs/specs/rhei-complete.spec.md`
- `docs/specs/rhei-reset.spec.md`
- `docs/specs/rhei-list.spec.md`
- `docs/specs/rhei-states.spec.md`
- `docs/specs/rhei-viz.spec.md`
- relevant command-surface sections in `docs/specs/rhei-usage.spec.md`

Implementation scope:

- command handlers in `crates/rhei-cli/src/main.rs`
- output rendering helpers in `crates/rhei-output/src/`
- e2e and integration tests for `next`, `transition`, `complete`, `reset`,
  `list`, `states`, and `viz`

Focus on claimability, assignee behavior, state instructions, transition
authorization, completion result files, reset semantics, list output, state
inspection, graph rendering, and CLI diagnostics.
