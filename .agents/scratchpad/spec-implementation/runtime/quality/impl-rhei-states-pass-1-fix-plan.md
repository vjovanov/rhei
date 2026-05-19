# Quality Fix Plan pass 1

## Accepted

- Q-poll-max-attempts-off-by-one: Poll self-loops stop one attempt early for `max_attempts > 2`.
  - Files: `crates/rhei-cli/src/cli/metadata_conditions.rs`, `crates/rhei-cli/src/cli/transition_context.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_programs_callbacks.rs`
  - Approach: Make poll self-loop eligibility compare the current 1-based attempt count against `poll.max_attempts` directly, so a task may self-loop while the current attempt is below the cap and is blocked only when `stateVisits.<state> >= poll.max_attempts`. Keep `record_poll_self_loop_if_needed` as the single place that persists the next attempt count, unless the implementation needs a small shared helper to compute the current poll attempt consistently.
  - Tests/checks: Add a regression with `max_attempts: 3` that records three program attempts, proves the second self-loop is allowed, and proves the third failed attempt routes to an exhaustion transition. Run `cargo test -p rhei-cli --test integration_markdown_plans run_poll`.

- Q-program-poll-bypasses-normal-transition-selection: Program-backed polling does not evaluate condition-only poll transitions after a successful program exit.
  - Files: `crates/rhei-cli/src/cli/programs.rs`, `crates/rhei-cli/src/cli/run_agent_mode.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_programs_callbacks.rs`
  - Approach: Replace the successful-program path's exit-code-only lookup with transition-order selection that accepts a rule when its `exit_code` matches the actual exit code or when it has no `exit_code` and its normal applicability checks pass. Preserve exact numeric/list exit-code precedence over `nonzero` for non-zero exits, preserve output validation before successful non-self-loop advancement, and ensure selected poll self-loops still flow through `record_poll_self_loop_if_needed`.
  - Tests/checks: Add a program-backed poll test whose self-loop has no `exit_code` and whose exhaustion route is `condition: pollAttempts >= pollMaxAttempts`; verify `rhei run` schedules retry metadata on the first pass and reaches exhaustion after the configured attempt cap. Run `cargo test -p rhei-cli --test integration_markdown_plans run_poll`.

## Rejected / Deferred

None.
