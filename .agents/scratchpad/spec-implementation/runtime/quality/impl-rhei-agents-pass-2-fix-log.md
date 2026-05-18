# Quality Fix Log pass 2

## Q-defaults-only-agent-mode-skipped

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/main_parts/run_command.rs`
  - `crates/rhei-cli/src/main_parts/ready_transition.rs`
  - `crates/rhei-cli/src/main_parts/tests_5.rs`
- Summary: mode selection now checks effective per-state agent invocations, so CLI overrides, `defaults.agent`, `defaults.model`, and `models.<id>.default_agent` can enter agent mode even when the state itself declares no agent fields.
- Checks:
  - `cargo test -p rhei-cli defaults_only_agent_mode` skipped because `rhei-cli` tests do not compile before selected tests run; the blocker was observed while attempting `cargo test -p rhei-cli mode_default_order`: `crates/rhei-cli/src/main_parts/tests_1.rs` ends with an unclosed delimiter at the include boundary.
  - Covered by production checks listed below.

## Q-missing-outputs-respawn-loop

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/main_parts/run_agent_mode.rs`
  - `crates/rhei-cli/src/main_parts/run_callback_mode.rs`
  - `crates/rhei-cli/src/main_parts/tests_6.rs`
- Summary: successful agent exits with missing required outputs now emit the required-output warning for the just-completed invocation and stop that attempt before the pending-invocation continuation path can reschedule it in the same run loop.
- Checks:
  - `cargo test -p rhei-cli missing_outputs_reschedule` skipped after the same `rhei-cli` test-harness compile blocker above.
  - Covered by production checks listed below.

## Q-optional-tooling-disappears

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/main_parts/agent_command.rs`
  - `crates/rhei-cli/src/main_parts/tests_5.rs`
- Summary: optional unavailable MCP/skill entries remain in resolved tooling with unavailable status for prompt/env/log visibility, while command attachment flags and generated MCP config still filter them out.
- Checks:
  - `cargo test -p rhei-cli optional_tooling_availability` skipped after the same `rhei-cli` test-harness compile blocker above.
  - Covered by production checks listed below.

## Q-target-selector-overridden-by-registry

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/main_parts/agent_resolution.rs`
  - `crates/rhei-cli/src/main_parts/tests_3.rs`
- Summary: `target`/`all_targets` now preserve their literal provider and model as the effective provider/model values, while still consulting a same-named registry model only for per-agent binding metadata.
- Checks:
  - `cargo test -p rhei-cli target_selector_literal_model` skipped after the same `rhei-cli` test-harness compile blocker above.
  - Covered by production checks listed below.

## Q-mode-default-order-lost

- Status: applied
- Files edited:
  - `crates/rhei-validator/src/lib_parts/preamble.rs`
  - `crates/rhei-validator/src/lib_parts/tests_profiles.rs`
  - `crates/rhei-cli/src/main_parts/cli_1.rs`
  - `crates/rhei-cli/src/main_parts/settings_1.rs`
  - `crates/rhei-cli/src/main_parts/tests_3.rs`
- Summary: custom agent profile `modes` now deserialize into insertion order, so fallback mode selection uses declaration order instead of lexical key order.
- Checks:
  - `cargo test -p rhei-cli mode_default_order` attempted, but blocked by the `rhei-cli` test-harness compile issue above.
  - `cargo test -p rhei-cli-validator custom_agent_profile` passed.
  - Covered by production checks listed below.

## Q-tooling-unknown-id-not-validation

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/main_parts/settings_2.rs`
  - `crates/rhei-cli/src/main_parts/tests_4.rs`
- Summary: id-only MCP/skill references in defaults and state definitions now validate against the merged registries; known registry entries whose concrete skill path is unavailable remain runtime availability statuses.
- Checks:
  - `cargo test -p rhei-cli unknown_tooling_id_validation` skipped after the same `rhei-cli` test-harness compile blocker above.
  - Covered by production checks listed below.

## Shared Checks

- `cargo fmt --all -- --check` passed.
- `cargo build -p rhei-cli` passed.
- `cargo clippy -p rhei-cli --bin rhei -- -D warnings -W clippy::all` passed.
- `cargo test -p rhei-cli-validator custom_agent_profile` passed.

## Intentionally Left Unfixed

- The existing `rhei-cli` unit-test include boundary issue was left unfixed because it is outside the accepted fix scope: `crates/rhei-cli/src/main_parts/tests_1.rs` ends mid-function, causing `cargo test -p rhei-cli ...` to fail with an unclosed delimiter before any selected test runs.
