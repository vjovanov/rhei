### Task apply-decision: Apply the converged merge policy
**State:** apply
**Prior:** Task discussion-seed

Read the consensus in `runtime/discussion/decision.md` and apply the agreed
policy. Record what was applied to `runtime/discussion/applied.md`.

This task is blocked until `discussion-seed` reaches a terminal decision — that is
how a discussion gates downstream work. If the discussion escalates instead of
converging, this task stays blocked until a human resolves the gate.
