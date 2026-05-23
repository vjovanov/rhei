### Task human-gate: Seed a visible human gate
**State:** human-gate
**Assignee:** ui-reviewer

Tests: a `gating` state that stops autonomous progress until a human transitions
it to `completed` or `cancelled`, plus `**Assignee:**` rendering (the gate is
owned by a human, so the orchestrator never auto-claims it).

### Task blocked-seeded: Seed a static blocked row
**State:** blocked

Tests: static rendering of the non-terminal `blocked` state, for contrast with
the live `live-failure-blocked` and `poll-exhaustion` tasks.

### Task terminal-completed: Seed completed terminal work
**State:** completed

Tests: terminal `completed` row rendering in static and live views.

### Task terminal-cancelled: Seed cancelled terminal work
**State:** cancelled

Tests: terminal `cancelled` row rendering in static and live views.