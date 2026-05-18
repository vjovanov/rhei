### Task coordinate: Resolve spec set and fan out per-spec + e2e-aggregate tasks
**State:** coordinate

Resolve the input mode and the concrete spec set:

{% if spec_path and not spec_ref -%}
Single-spec mode is active. The spec set is exactly one file: `{{spec_path}}`.
{%- elif spec_ref and not spec_path -%}
Multi-spec mode is active. Resolve `{{spec_ref}}` to the changed `*.spec.md`
files using the project's VCS tooling (`git diff`, `gh`, etc.). Every changed
spec file becomes one per-spec implementation task.
{%- elif spec_path and spec_ref -%}
ERROR: both `spec_path` and `spec_ref` are set at instantiation. Stop and ask
the operator which mode they want.
{%- else -%}
ERROR: neither `spec_path` nor `spec_ref` is set at instantiation. Stop and
ask the operator to supply one when instantiating.
{%- endif %}

For every spec in scope, append one `impl-<slug>` task to `tasks/` with
`**State:** implement` and `**Prior:** Task coordinate`, and write a
single-line `runtime/manifests/<task_id>-spec.txt` containing that spec's
repo-relative path.

Append exactly one `e2e-aggregate` task with `**State:** e2e-write` and
`**Prior:**` listing every `impl-<slug>` task you created. This task drives
the shared end-to-end coverage loop after every per-spec task completes.

Write the assignments manifest (per-task slug + spec path) to
`runtime/manifests/coordinate-spec-assignments.md`. Transition to `completed`
once the manifest, per-task spec files, per-spec tasks, and the e2e-aggregate
task are all in place.
