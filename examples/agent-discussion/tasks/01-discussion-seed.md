### Task discussion-seed: Decide how a converged discussion enters the plan
**State:** collect

**The point (D-merge-policy):** When an agent discussion converges on a decision,
how should that decision enter the plan — auto-merge, a recorded judge ruling, or
human escalation?

Four participants discuss this point, each arguing from the project goal they
champion:

- **claude — Developer Experience:** coordination should stay frictionless and
  human-legible; gating every decision kills flow.
- **codex — Determinism & Auditability:** every decision must be reproducible and
  recorded; nothing should merge silently.
- **gemini — Throughput & Scale:** never put a human in the hot path of a parallel
  swarm; blocking serializes everything.
- **cursor — Safety & Human Oversight:** irreversible or destructive decisions
  must pass a human gate.

**How the discussion runs:**

- Round 1: each participant states its opening position from its goal (blind to
  the others).
- Round 2+: each participant reads the previous round's digest and responds to the
  others by name — conceding and sharpening — so the positions actually move.
- The judge writes a per-round digest and rules: converged, another round, or
  (when the round budget is exhausted) escalate to a human.

The converged decision is recorded in `runtime/discussion/decision.md`, which the
`apply-decision` task consumes.
