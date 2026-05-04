### Task state-machines-transitions: Audit state machine and transition semantics
**State:** scope-spec

Audit state-machine YAML semantics and explicit transition behavior.

Spec scope:

- `docs/specs/rhei-states.spec.md`
- `docs/specs/rhei-transitions.spec.md`
- `docs/specs/rhei-state-machine-writer.spec.md`

Implementation scope:

- `crates/rhei-validator/src/`
- transition validation and execution code in `crates/rhei-cli/src/main.rs`
- state rendering and state-machine resolution in `crates/rhei-cli/src/main.rs`
- state-machine related integration and e2e tests

Focus on profiles, node policy, state fields, terminal and gating behavior,
artifact contracts, callbacks, counted visits, polling, concurrent state
scheduling metadata, and transition selection/validation semantics.
