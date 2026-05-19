# Quality Fix Plan pass 2

## Accepted

- Q-profile-allowed-not-enforced-on-transition: Runtime transition paths can move a node into a state excluded by its resolved `node_policy` profile.
  - Files: `crates/rhei-cli/src/cli/system_transition_execution.rs`, `crates/rhei-cli/src/cli/ready_transition.rs`, `crates/rhei-cli/tests/integration_markdown_plans/states.rs`
  - Approach: Resolve the target task's node policy profile from its kind and depth before selecting or committing a transition, then reject or filter any destination state that is not in the resolved profile's `allowed` set. Apply the same check after callback redirect resolution so a callback cannot redirect into a profile-disallowed state. Keep global state/edge validation intact; the profile check is an additional per-node guard.
  - Tests/checks: Add regression coverage with a profile that excludes an otherwise globally valid transition target, asserting `rhei transition` rejects that destination and automatic transition selection skips or fails instead of writing an invalid state. Run `cargo test -p rhei-cli --test integration_markdown_plans states`.

## Rejected / Deferred

None.
