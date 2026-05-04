### Task run-orchestration-agents-programs: Audit run orchestration, agents, programs, and callbacks
**State:** scope-spec

Audit autonomous execution semantics for `rhei run`.

Spec scope:

- `docs/specs/rhei-run.spec.md`
- `docs/specs/rhei-agents.spec.md`
- `docs/specs/rhei-programs.spec.md`
- `docs/specs/rhei-callbacks.spec.md`
- relevant workflow sections in `docs/specs/rhei-usage.spec.md`

Implementation roots:
- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Focus on ready-set selection, parallel scheduling, concurrent state handling,
gating barriers, subprocess completion authority, output artifact enforcement,
settings merge order, agent registry resolution, target selectors, timeouts,
callback invocation, and failure routing.