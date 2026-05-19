# Quality Fix Log pass 2

Task: impl-rhei-snapshots
State: quality-fix

## Q-stationary-exits-skip-emit

Status: accepted fix applied.

Files edited:
- `crates/rhei-cli/src/cli/run_agent_mode.rs`
- `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`

Changes:
- Emitted snapshots for successful agent exits that remain in the same state because required outputs are missing.
- Emitted snapshots for successful agent exits that select no outgoing transition.
- Added run-agent regression coverage for missing outputs and no-transition exits, asserting `_state` and named `failure` / `always` snapshots.
- Avoided an agent missing-output rerun loop by treating the blocked missing-output case as pass-ending rather than immediately starting another pass.

## Q-timeout-failure-emit-before-routing

Status: accepted fix applied with narrow scope.

Files edited:
- `crates/rhei-cli/src/cli/run_agent_mode.rs`
- `crates/rhei-cli/src/cli/run_failure_transitions.rs`
- `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`
- `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`

Changes:
- Added timeout transition selection before snapshot emission and applied the selected timeout route after emission.
- Passed the selected failure/timeout destination into `emit_snapshots_after_agent_exit`.
- Preserved poll self-loop suppression for success, failure, and timeout classifications.
- Added run-agent regression coverage for timeout routing and nonzero no-route failure emission.

Intentionally left unfixed:
- Valid agent nonzero exit-code routing is not covered because the current state-machine validator rejects `exit_code` transitions on non-program states. Adding a new agent error-route schema would exceed this accepted fix's scope.

## Q-pi-observed-model-not-parsed

Status: accepted fix applied.

Files edited:
- `crates/rhei-cli/src/cli/snapshot_records.rs`
- `crates/rhei-cli/src/cli/snapshot_runtime_emit.rs`
- `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`

Changes:
- Added a Pi JSONL header parser for observed provider/model values.
- Threaded observed provider/model into `write_snapshot_generation_atomic`.
- Logged a warning and fell back to declared provider/model when the Pi header is missing or unparsable.
- Added snapshot runtime tests for observed provider/model override and fallback behavior, including cache-benefit diagnostics.

## Q-invalid-session-layout-passes-validation

Status: accepted fix applied.

Files edited:
- `crates/rhei-cli/src/cli/settings_load_validate.rs`
- `crates/rhei-cli/src/cli/snapshot_records.rs`
- `crates/rhei-cli/src/cli/snapshot_runtime_preload.rs`
- `crates/rhei-cli/src/cli/tests_agent_execution_validation.rs`

Changes:
- Replaced loose snapshot session validation with shared runtime-aligned support predicates.
- Required supported `FlatById` layout metadata and a session output source for named emit.
- Required supported layout plus concrete resume or fork flag for required preload.
- Added validation tests for unsupported layout kind, incomplete layout, and non-empty but unsupported resume objects.

## Checks Run

Passed:
- `cargo fmt --all -- --check`
- `cargo check -p rhei-cli --bin rhei`
- `cargo test -p rhei-cli --test integration_markdown_plans run_agent`

Attempted but blocked:
- `cargo test -p rhei-cli --bin rhei snapshot`
- `cargo test -p rhei-cli --bin rhei agent_execution_validation`

Both unit-test commands currently fail to compile before reaching the filtered tests because `crates/rhei-cli/src/cli/tests_settings_tooling.rs` still calls `should_use_agent_mode` with the old 3-argument signature at lines 376, 381, 395, and 408. That file is outside the accepted pass-2 fix scope, so it was left unchanged.

## Workflow

This is pass 2 of 2. No `rhei transition`, `rhei complete`, or direct `**State:**` edit was performed; the spawning `rhei run` process is responsible for advancing the task state.
