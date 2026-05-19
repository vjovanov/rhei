# Quality Fix Plan pass 2

## Accepted

- Q-stationary-exits-skip-emit: Successful agent subprocess exits can skip both auto `_state` and named snapshot emission when the task remains in the same state.
  - Files: `crates/rhei-cli/src/cli/run_agent_mode.rs`, `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`
  - Approach: In both sequential and parallel agent-run paths, ensure snapshot emission is driven by subprocess completion after completion classification and transition selection, not only by a state transition being applied. Emit snapshots for successful exits with missing required outputs and for successful exits where no outgoing transition is selected; keep existing explicit suppression cases such as poll self-loops and non-agent states.
  - Tests/checks: Add or extend run-agent regression coverage for a successful exit with missing required outputs and a successful no-transition exit, asserting the expected `_state` and named `failure`/`always` generations are written; run `cargo test -p rhei-cli --bin rhei snapshot` and `cargo test -p rhei-cli --test integration_markdown_plans run_agent`.

- Q-timeout-failure-emit-before-routing: Failure and timeout snapshots are emitted before error/timeout routing selects the outgoing transition.
  - Files: `crates/rhei-cli/src/cli/run_agent_mode.rs`, `crates/rhei-cli/src/cli/run_failure_transitions.rs`, `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`
  - Approach: Route non-zero exits and timeouts through the same transition-selection step used for successful auto-advance before calling `emit_snapshots_after_agent_exit`. Pass the selected destination state into the emit hook, then apply the selected failure/timeout route. Preserve poll self-loop suppression and make terminal error/timeout routes visible to the snapshot manifest path.
  - Tests/checks: Add focused coverage for failure/timeout routing that asserts snapshot emission sees the selected route, and add a poll timeout/error self-loop case that asserts no attempt snapshot is written; run `cargo test -p rhei-cli --bin rhei snapshot` and `cargo test -p rhei-cli --test integration_markdown_plans run_agent`.

- Q-pi-observed-model-not-parsed: Snapshot manifests do not parse Pi session headers for observed provider/model.
  - Files: `crates/rhei-cli/src/cli/snapshot_records.rs`, `crates/rhei-cli/src/cli/snapshot_runtime_emit.rs`, `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`
  - Approach: Add a Pi-specific JSONL header parser at emit time for supported Pi session layouts, returning observed provider/model when present. Thread those observed values into `write_snapshot_generation_atomic` instead of deriving them only from the resolved target tuple, and log a warning while falling back to declared provider/model when the Pi header is missing or unparsable.
  - Tests/checks: Add snapshot runtime tests with a Pi transcript header whose observed provider/model differ from the declared selector, plus missing/unparsable header fallback coverage; assert `manifest.json` stores the observed values and cache-benefit diagnostics compare against them. Run `cargo test -p rhei-cli --bin rhei snapshot`.

- Q-invalid-session-layout-passes-validation: Static validation accepts snapshot session profiles that runtime later rejects as unsupported.
  - Files: `crates/rhei-cli/src/cli/settings_load_validate.rs`, `crates/rhei-cli/src/cli/snapshot_records.rs`, `crates/rhei-cli/src/cli/snapshot_runtime_preload.rs`, `crates/rhei-cli/src/cli/tests_agent_execution_validation.rs`
  - Approach: Replace the loose validation predicates with the same support checks runtime uses: require a recognized session layout kind with the required extension/source fields for statically resolved named emits, and require a concrete supported resume or fork flag for required preload. Share a small predicate/helper if needed so validation and runtime cannot drift.
  - Tests/checks: Extend agent execution validation tests with unsupported layout kind, incomplete layout, and non-empty but unsupported resume objects; assert `rhei validate` reports `unsupported-snapshot-session`. Run `cargo test -p rhei-cli --bin rhei agent_execution_validation`.

## Rejected / Deferred

None.
