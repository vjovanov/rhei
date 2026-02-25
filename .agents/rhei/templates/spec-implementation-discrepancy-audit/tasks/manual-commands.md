### Task manual-commands: Audit manual worker and inspection commands
**State:** scope-spec

Audit the command contracts used by humans and manual agents outside the full
`rhei run` orchestrator.

Spec scope:

- `{{spec_root}}/specs/rhei-next.spec.md`
- `{{spec_root}}/specs/rhei-transition-cmd.spec.md`
- `{{spec_root}}/specs/rhei-complete.spec.md`
- `{{spec_root}}/specs/rhei-reset.spec.md`
- `{{spec_root}}/specs/rhei-list.spec.md`
- `{{spec_root}}/specs/rhei-states.spec.md`
- `{{spec_root}}/specs/rhei-viz.spec.md`
- relevant command-surface sections in `{{spec_root}}/specs/rhei-usage.spec.md`

Implementation roots:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}

Focus on claimability, assignee behavior, state instructions, transition
authorization, completion result files, reset semantics, list output, state
inspection, graph rendering, and CLI diagnostics.
