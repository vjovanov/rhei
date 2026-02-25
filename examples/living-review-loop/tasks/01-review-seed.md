### Task review-seed: Multi-model review of the rhei CLI implementation
**State:** review

Review the rhei specification documents in `docs/specs/` for gaps, contradictions,
and ambiguities. Focus on problems that would mislead an implementor or confuse a
user — not style or speculative additions.

**What to examine:**

- Internal consistency: check that terms, field names, and behaviour are defined
  the same way across all spec files
- Completeness: identify any behaviour that is referenced but not specified, or
  any state/transition property that lacks a clear rule
- Accuracy: flag any spec claim that contradicts observable CLI or runtime behaviour

**Review phase (all models — claude, codex):**
Each model reads the spec files in `docs/specs/` and writes its findings to
`runtime/findings/<model>-findings.md`. Record only observations that are distinct
from what the other model is likely to note. Be specific: name the file, section,
and the exact inconsistency or gap. Transition to `consolidate` when your findings
file is written.

**Consolidate phase (orchestrator):**
`codex` reads both findings files. Write a deduplicated consolidated summary to
`runtime/findings/review-findings.md`. Append one `prove` task per distinct
review point. Transition to `completed` when all verification tasks are appended.

**Prove phase (one task per finding, using codex only):**
Each prove task reads its assigned finding, verifies it against the spec and the
codebase with `codex`, and writes a structured result to
`runtime/verifications/<finding-id>.md`:

- Confirmed: yes/no
- Actionable: yes/no
- Summary: one-sentence explanation

If the finding is confirmed and actionable, append a fix task to `tasks/`.
Transition to `completed` when the verification file is written.
