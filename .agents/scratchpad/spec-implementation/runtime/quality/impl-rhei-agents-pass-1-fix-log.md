# Quality Fix Log pass 1

## Applied

- Q-settings-parse-swallowed
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Change: Settings file loading now treats missing files as defaults but reports read, JSON parse, and typed decode failures with the offending path. Runtime call sites that can fail now propagate settings errors; completion and post-spawn warning helpers degrade explicitly.
  - Checks: `cargo test -p rhei-cli --bin rhei settings`; `cargo check -p rhei-cli --bin rhei`; `cargo fmt --all -- --check`.

- Q-required-tooling-still-spawns
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Change: Added a pre-spawn tooling gate for unresolved entries, missing skill paths, and unsupported agent wiring. Optional failures warn and are dropped. Required failures block spawn and attempt `mcp_unavailable` / `skill_unavailable` system transitions with `transitionData.unavailable` and `transitionData.kind`; otherwise they log/error consistently with `--continue-on-error`.
  - Checks: `cargo test -p rhei-cli --bin rhei tooling`; `cargo check -p rhei-cli --bin rhei`; `cargo fmt --all -- --check`.

- Q-run-ignores-claim-ownership
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Change: Added a run-specific runnable-task helper that preserves run readiness semantics while excluding tasks with `assignee.is_some()`. `rhei next` claimability remains separate.
  - Checks: `cargo test -p rhei-cli --bin rhei assigned`; `cargo check -p rhei-cli --bin rhei`; `cargo fmt --all -- --check`.

- Q-null-does-not-clear
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Change: Settings merge now consults raw JSON field presence so omitted project fields inherit while explicit `null` clears optional inherited fields. Covered nested defaults, top-level legacy optional defaults, model `default_agent`, and model-agent binding `timeout`.
  - Checks: `cargo test -p rhei-cli --bin rhei merge`; `cargo test -p rhei-cli --bin rhei settings`; `cargo check -p rhei-cli --bin rhei`; `cargo fmt --all -- --check`.

- Q-model-registry-required
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Change: Legacy model resolution now requires any resolved model id from CLI, state, `all_models`, defaults, or top-level settings to exist in `settings.models`. No-model configurations still resolve without model context; target selectors remain separate.
  - Checks: `cargo test -p rhei-cli --bin rhei model_registry`; `cargo check -p rhei-cli --bin rhei`; `cargo fmt --all -- --check`.

- Q-parallel-timeout-data-lost
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Change: `AgentSpawnOutcome` now carries the resolved timeout seconds, and the parallel timeout path passes that value into `fire_timeout_transition` so `transitionData.timeout` uses the resolved duration.
  - Checks: `cargo test -p rhei-cli --bin rhei timeout`; `cargo check -p rhei-cli --bin rhei`; `cargo fmt --all -- --check`.

## Checks Run

- `cargo fmt --all -- --check` - passed
- `cargo check -p rhei-cli --bin rhei` - passed
- `cargo test -p rhei-cli --bin rhei settings` - passed
- `cargo test -p rhei-cli --bin rhei tooling` - passed
- `cargo test -p rhei-cli --bin rhei assigned` - passed
- `cargo test -p rhei-cli --bin rhei merge` - passed
- `cargo test -p rhei-cli --bin rhei model_registry` - passed
- `cargo test -p rhei-cli --bin rhei timeout` - passed

## Intentionally Left Unfixed

- Q-mcp-flag-value-not-launch-spec remains deferred per the fix plan.
- Q-parallel-waits-for-batch remains deferred per the fix plan.
