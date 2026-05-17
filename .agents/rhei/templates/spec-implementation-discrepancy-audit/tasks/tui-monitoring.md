### Task tui-monitoring: Audit run TUI, journal, and live monitoring behavior
**State:** scope-spec

Audit the monitoring and terminal UI surface for running plans.

Spec scope:

- `{{spec_root}}/rhei-run-tui.spec.md`
- TUI and journal requirements referenced by `{{spec_root}}/rhei-run.spec.md`

Implementation roots:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}

Focus on event semantics, transition journal format, stdout compatibility,
agent traffic interception, lifecycle event preservation, log tailing, slot
layout, non-TTY behavior, `--tui` / `--no-tui`, and failure/timeout display.
