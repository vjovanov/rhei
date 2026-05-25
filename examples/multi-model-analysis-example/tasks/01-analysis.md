### Task analysis: Multi-target analysis — Compare error-handling strategies across the runtime
**State:** analyze

Analyze how the Rhei runtime distinguishes recoverable from unrecoverable
failures during `rhei run`: where errors abort the orchestrator, where they
are surfaced as warnings, and where work is retried or skipped. Identify the
three weakest points in the current approach and, for each, name the file and
the concrete failure mode. Stay scoped to the runtime crates; do not propose
a redesign.


Write one analysis note per target under `runtime/analyses/`, then
write the final synthesized document to `runtime/final-analysis.md`.
