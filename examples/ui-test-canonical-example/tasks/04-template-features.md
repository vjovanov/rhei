### Task scenario-dashboard-checkout-flow: Pin the scenario slug into a task id
**State:** completed

Tests: the `| slug` instantiation filter, turning `dashboard checkout flow` into a
filesystem- and id-safe segment used directly in this task's id.


### Task followup-preview: Mirror the runtime-generated follow-up shape
**State:** completed

Tests: instantiation-time `{% if %}` conditional inclusion
— this task is emitted only when `include_generated_followup` is true, mirroring
the task that the `aggregate` transition callback appends at runtime.
