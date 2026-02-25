### Task run-orchestration-agents-programs: Audit run orchestration, agents, programs, and callbacks
**State:** scope-spec

Audit autonomous execution semantics for `rhei run`.

Spec scope:

- `docs/specs/rhei-run.spec.md`
- `docs/specs/rhei-agents.spec.md`
- `docs/specs/rhei-programs.spec.md`
- `docs/specs/rhei-callbacks.spec.md`
- relevant workflow sections in `docs/specs/rhei-usage.spec.md`

Implementation scope:

- run loop, scheduling, settings, agent resolution, program execution, callback
  resolution, timeout handling, and environment setup in `crates/rhei-cli/src/main.rs`
- run e2e tests and fixtures under `crates/rhei-cli/tests/e2e/`
- example workspaces under `examples/`

Focus on ready-set selection, parallel scheduling, concurrent state handling,
gating barriers, subprocess completion authority, output artifact enforcement,
settings merge order, agent registry resolution, target selectors, timeouts,
callback invocation, and failure routing.
