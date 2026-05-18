# Quality Fix Plan pass 1

## Accepted

- Q-program-timeout-misroutes-nonzero: Program failures are treated as timeouts whenever a timeout is configured.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Change program execution to return an outcome that distinguishes `status` from `timed_out`, then route timeout transitions only when the watchdog actually fired. Preserve normal `exit_code` transition handling for fast non-zero exits under a configured `program_timeout`.
  - Tests/checks: Add or update CLI coverage for a program task with `program_timeout` and a fast non-zero exit that must route by `exit_code`, plus a real timeout case that must route by `timeout`. Run `cargo test -p rhei-cli --test integration_markdown_plans` or the narrow test target added for these cases.

- Q-failure-transitions-require-success-outputs: Timeout and non-zero failure transitions are blocked by required output artifacts.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Thread transition purpose through transition execution closely enough to skip source-state `outputs:` validation for timeout, tooling-unavailable, and failed/non-zero completion routes while preserving the check for successful completion and normal transitions.
  - Tests/checks: Add or update coverage where a state declares required success outputs, the program/agent fails or times out without producing them, and the configured failure transition still applies. Run `cargo test -p rhei-cli --test integration_markdown_plans`.

- Q-poll-max-attempts-off-by-one: Poll `max_attempts` allows one extra subprocess attempt.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Fix poll self-loop eligibility so the subprocess attempt that just completed counts against `poll.max_attempts` before selecting another self-loop, either by comparing `current + 1` to the limit or by recording the current attempt before transition applicability is evaluated.
  - Tests/checks: Add or update a poll test with `max_attempts: 1` proving only one subprocess execution occurs and exhaustion/error routing happens on the next decision. Run `cargo test -p rhei-cli --test integration_markdown_plans`.

- Q-program-states-ignore-concurrent-flag: Program states bypass the concurrent-state scheduling rule.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Apply the existing non-concurrent per-state claimant/defer behavior before execution-kind splitting or add equivalent filtering to program tasks, so `concurrent: false` allows at most one ready program task per state per pass.
  - Tests/checks: Add or update coverage with multiple ready program tasks in the same default non-concurrent state and verify only one runs in a pass, with the others left ready/deferred. Run `cargo test -p rhei-cli --test integration_markdown_plans`.

## Rejected / Deferred

None.
