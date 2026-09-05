### Task scenario-{{ scenario_name | slug }}: Pin the scenario slug into a task id
**State:** script-check

Tests: the `| slug` instantiation filter, turning `{{ scenario_name }}` into a
filesystem- and id-safe segment used directly in this task's id.

{% if include_generated_followup %}
### Task followup-preview: Mirror the runtime-generated follow-up shape
**State:** script-check

Tests: instantiation-time `{% raw %}{% if %}{% endraw %}` conditional inclusion
— this task is emitted only when `include_generated_followup` is true, mirroring
the task that the `aggregate` transition callback appends at runtime.
{% endif %}
