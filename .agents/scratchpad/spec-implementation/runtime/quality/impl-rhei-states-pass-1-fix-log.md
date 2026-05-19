# Quality Fix Log pass 1

## Q-poll-max-attempts-off-by-one

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/cli/metadata_conditions.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans/run_programs_callbacks.rs`
- Notes:
  - Poll self-loop eligibility now compares the current 1-based poll attempt count directly with `poll.max_attempts`.
  - `record_poll_self_loop_if_needed` in `crates/rhei-cli/src/cli/transition_context.rs` remains the single place that persists the next attempt count; no edit was needed there.
  - Added a regression proving `max_attempts: 3` permits two self-loops and routes the third failed attempt to exhaustion after three program attempts.

## Q-program-poll-bypasses-normal-transition-selection

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/cli/programs.rs`
  - `crates/rhei-cli/src/cli/run_agent_mode.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans/run_programs_callbacks.rs`
- Notes:
  - Program exit transition selection now considers condition-only transitions in declaration order when no `exit_code` is declared.
  - Non-zero exits still preserve exact numeric/list `exit_code` precedence over `exit_code: nonzero`.
  - Successful non-self-loop program advancement still validates required outputs before applying the transition.
  - Selected poll self-loops still flow through `record_poll_self_loop_if_needed`.
  - Added a regression proving a successful program-backed poll can self-loop and exhaust through condition-only transitions using `pollAttempts` and `pollMaxAttempts`.

## Checks

- `cargo fmt --all -- --check` - passed
- `cargo test -p rhei-cli --test integration_markdown_plans run_poll` - passed (4 tests)
- Extra focused check: `cargo test -p rhei-cli --test integration_markdown_plans run_program` - passed (3 tests)

## Intentionally Left Unfixed

- None. Only accepted fixes from `runtime/quality/impl-rhei-states-pass-1-fix-plan.md` were applied.
