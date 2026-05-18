# Quality Fix Log pass 1

## Applied

- Q-program-timeout-misroutes-nonzero
  - Files edited: `crates/rhei-cli/src/main_parts/programs.rs`, `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/src/main_parts/system_transitions_1.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Change: program execution now returns a `timed_out` outcome separately from process status; timeout transitions fire only when the watchdog fired. Fast non-zero program exits with a configured `program_timeout` now continue through exit-code transition matching.

- Q-failure-transitions-require-success-outputs
  - Files edited: `crates/rhei-cli/src/main_parts/system_transitions_1.rs`, `crates/rhei-cli/src/main_parts/system_transitions_2.rs`, `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Change: transition origin now carries whether source success outputs should be skipped. Timeout, tooling-unavailable, and non-zero program exit-code transitions skip source `outputs:` validation; zero/success program transitions and normal/manual transitions still enforce source outputs.

- Q-poll-max-attempts-off-by-one
  - Files edited: `crates/rhei-cli/src/main_parts/metadata_1.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Change: poll self-loop applicability now counts the subprocess attempt that just completed before comparing to `poll.max_attempts`, preventing an extra retry when `max_attempts: 1`.

- Q-program-states-ignore-concurrent-flag
  - Files edited: `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Change: program tasks now use the existing non-concurrent per-state claimant/defer rule before execution, so only one ready task per default non-concurrent program state is scheduled per pass.

## Checks

- `cargo fmt --all -- --check` — passed.
- `cargo test -p rhei-cli --test integration_markdown_plans` — passed (75 tests).
- `cargo clippy -p rhei-cli --test integration_markdown_plans -- -D warnings -W clippy::all` — passed.

## Left Unfixed

- None. All accepted pass-1 fixes were applied. Existing unrelated working-tree changes were left intact.
