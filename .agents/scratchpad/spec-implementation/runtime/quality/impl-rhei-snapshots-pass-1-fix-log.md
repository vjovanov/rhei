# Quality Fix Log pass 1

Task: impl-rhei-snapshots
State handled: quality-fix

## Applied Fixes

- Q-targetless-auto-hard-error
  - Files edited:
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_2.rs`
    - `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Summary: targetless states without authored snapshot preload/emit now run cold and skip auto `_state` emission with an info diagnostic. Explicit `snapshot.emit` and `snapshot.inherit` still fail with `snapshot-requires-target`.

- Q-emit-log-not-session
  - Files edited:
    - `crates/rhei-cli/src/main_parts/snapshots_1.rs`
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`
    - `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Summary: snapshot emit no longer falls back to the Rhei run log. Emit now requires a staged native `FlatById` transcript, derives `manifest.session_id` from the native filename stem, and writes the layout extension as the snapshot transcript extension. Missing native transcripts skip auto emit and fail matching named emit with `unsupported-snapshot-session`.

- Q-poll-self-loop-emits
  - Files edited:
    - `crates/rhei-cli/src/main_parts/run_agent_mode.rs`
    - `crates/rhei-cli/src/main_parts/ready_transition.rs`
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`
    - `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Summary: successful agent snapshot emission is delayed until an outgoing transition is selected and before it is applied. Poll self-loop selections suppress both auto and named snapshot emission; terminal poll exits still emit.
  - Note: `ready_transition.rs` was edited narrowly to pass a pre-transition emit hook at the existing transition-selection point.

- Q-ancestor-select-state-order
  - Files edited:
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`
    - `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Summary: ancestor inheritance now applies `snapshot.inherit.select.state` while walking ancestors, so the nearest ancestor is chosen only if it has an orchestrator snapshot matching the inherited name and selected emitting state.

## Checks

- `cargo fmt --all`
  - Result: passed.

- `cargo fmt --all -- --check`
  - Result: passed.

- `cargo check -p rhei-cli --bin rhei`
  - Result: passed.

- `cargo test -p rhei-cli --bin rhei snapshot`
  - Result: blocked before snapshot tests compiled.
  - Blocker: unrelated existing syntax error in `crates/rhei-cli/src/main_parts/tests_1.rs` at `path_matches_normalizes_paths` (`unclosed delimiter`, around lines 493-499).

- `cargo test -p rhei-cli --bin rhei snapshot poll`
  - Result: not run separately because the same test harness parse error blocks all `rhei` binary tests.

- `cargo test -p rhei-cli --bin rhei snapshot_inherit`
  - Result: not run separately because the same test harness parse error blocks all `rhei` binary tests.

## Intentionally Left Unfixed

- The unrelated `tests_1.rs` unclosed delimiter is outside the accepted fix plan and was left unchanged.
- No rejected or deferred quality items were present in the pass-1 fix plan.

## Next State

`quality-fix` visit count is 1 and the loop budget is 2, so the Rhei runner should transition this task back to `quality-review` after this invocation.
