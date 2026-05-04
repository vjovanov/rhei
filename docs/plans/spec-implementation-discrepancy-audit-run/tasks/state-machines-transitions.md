### Task state-machines-transitions: Audit state machine and transition semantics
**State:** human-decision

Audit state-machine YAML semantics and explicit transition behavior for
Rhei.

Spec scope:

- `docs/specs/rhei-states.spec.md`
- `docs/specs/rhei-transitions.spec.md`
- `docs/specs/rhei-state-machine-writer.spec.md`

Implementation roots:
- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Focus on profiles, node policy, state fields, terminal and gating behavior,
artifact contracts, callbacks, counted visits, polling, concurrent state
scheduling metadata, and transition selection/validation semantics.