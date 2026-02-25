### Task run-orchestration-agents-programs: Audit run orchestration, agents, programs, and callbacks
**State:** scope-spec

Audit autonomous execution semantics for `rhei run`.

Spec scope:

- `{{spec_root}}/specs/rhei-run.spec.md`
- `{{spec_root}}/specs/rhei-agents.spec.md`
- `{{spec_root}}/specs/rhei-programs.spec.md`
- `{{spec_root}}/specs/rhei-callbacks.spec.md`
- relevant workflow sections in `{{spec_root}}/specs/rhei-usage.spec.md`

Implementation roots:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}

Focus on ready-set selection, parallel scheduling, concurrent state handling,
gating barriers, subprocess completion authority, output artifact enforcement,
settings merge order, agent registry resolution, target selectors, timeouts,
callback invocation, and failure routing.
