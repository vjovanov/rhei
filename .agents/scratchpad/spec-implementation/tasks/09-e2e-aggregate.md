### Task e2e-aggregate: Extend e2e coverage for implemented specs
**State:** pause
**Prior:** Task impl-rhei-agents, Task impl-rhei-run, Task impl-rhei-snapshot-operations, Task impl-rhei-snapshots, Task impl-rhei-states, Task impl-rhei-transitions, Task impl-rhei-usage

Paused before first execution. Resume to `e2e-write` after the per-spec tasks are ready.

Drive the shared end-to-end coverage loop for every spec listed in `runtime/manifests/coordinate-spec-assignments.md`. Enforce the mock-agent policy: every standard e2e test targets the mock agent (`mock`); a small release-only set marked with `release-only` may invoke real agents.
