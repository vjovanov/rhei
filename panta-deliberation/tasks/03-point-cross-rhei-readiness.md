### Task point-cross-rhei-readiness: Resolve Cross-rhei dependency readiness
**State:** completed
**Prior:** Task split

Resolve the readiness rule for dependencies that cross rhei boundaries without
changing unrelated hierarchy or state-machine default rules.

Source evidence:

> Cross-rhei dependencies must use the same readiness rule as normal
> scheduling: a prior must be in a successful terminal state, meaning
> `final: true` and not normalized `cancelled`.

> A cancelled prerequisite should not unblock dependent work.

Question:

What readiness rule should determine whether a cross-rhei dependency unblocks
dependent work?

Constraints:

- Cross-rhei dependencies must use the same readiness rule as normal
  scheduling.
- A prerequisite must be in a terminal state with `final: true` before it can
  unblock dependent work.
- A prerequisite whose normalized state is `cancelled` must not unblock
  dependent work.
