### Task state-machines-transitions: Audit state machine and transition semantics
**State:** scope-spec

Audit state-machine YAML semantics and explicit transition behavior for
{{subject}}.

Spec scope:

- `{{spec_root}}/rhei-states.spec.md`
- `{{spec_root}}/rhei-transitions.spec.md`
- `{{spec_root}}/rhei-state-machine-writer.spec.md`

Implementation roots:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}

Focus on profiles, node policy, state fields, terminal and gating behavior,
artifact contracts, callbacks, counted visits, polling, concurrent state
scheduling metadata, and transition selection/validation semantics.
