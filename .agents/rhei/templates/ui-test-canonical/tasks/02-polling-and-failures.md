### Task polling: Poll a mock external system until ready
**State:** script-poll

Tests: a `poll` state self-looping on exit 75 until the final attempt becomes
ready, then completing through a smoke check.

### Task poll-exhaustion: Exhaust poll attempts into blocked
**State:** poll-exhaust

Tests: poll `max_attempts` exhaustion routing to `blocked` via
`condition: pollAttempts >= pollMaxAttempts` when readiness never arrives.

### Task live-failure-blocked: Route a live program failure to blocked
**State:** script-fail

Tests: a failing program matched by an `exit_code: [1, 2, 42]` array driving a
live transition into `blocked`, so the UI renders a failure that happens during
the run rather than a seeded one.

### Task skill-unavailable-blocked: Route a missing required skill to blocked
**State:** agent-missing-skill

Tests: the `skill_unavailable` transition firing when a required skill
(`absent-lens`) is absent at spawn time.

### Task mcp-unavailable-blocked: Route an unavailable MCP server to blocked
**State:** agent-missing-mcp

Tests: the `mcp_unavailable` transition firing when a required MCP server
(`mock-mcp`) fails to start.
