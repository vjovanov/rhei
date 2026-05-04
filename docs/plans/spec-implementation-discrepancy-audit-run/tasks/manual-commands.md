### Task manual-commands: Audit manual worker and inspection commands
**State:** human-decision

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

Implementation roots:
- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Focus on claimability, assignee behavior, state instructions, transition
authorization, completion result files, reset semantics, list output, state
inspection, graph rendering, and CLI diagnostics.