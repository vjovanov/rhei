# Quality Fix Plan pass 1

## Accepted

- Q-targetless-auto-hard-error: Agent states without an authored snapshot operation can fail before spawn when no effective snapshot target tuple resolves.
  - Files: `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`, `crates/rhei-cli/src/main_parts/snapshot_runtime_2.rs`, `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Approach: In preload, determine whether the state declares `snapshot.inherit` or an override is present before requiring a target slug; return a default `SnapshotPreload` for states with no authored preload/override and no target. In emit, use optional target resolution for auto `_state` snapshots and skip auto-emit with an info diagnostic when no effective target tuple exists; keep explicit `snapshot.emit` and `snapshot.inherit` paths as hard `snapshot-requires-target` errors.
  - Tests/checks: Add focused runtime tests showing a snapshot-capable targetless agent state without `snapshot.emit`/`snapshot.inherit` runs without preload/emit failure, while explicit named emit or inherit still fails with `snapshot-requires-target`; run `cargo test -p rhei-cli --bin rhei snapshot`.

- Q-emit-log-not-session: Snapshot emit can cache the rhei run log as `transcript.log` and fabricate `manifest.session_id` instead of preserving the native agent session transcript.
  - Files: `crates/rhei-cli/src/main_parts/snapshots_1.rs`, `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`, `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Approach: Replace the log fallback with a real transcript requirement. Resolve the emitted transcript from the configured `SessionLayout` and staged `SnapshotPreload.session_dir`; for `FlatById`, select the newest `<id>.<ext>` file and derive `session_id` from the filename stem. If no native transcript is found, skip auto-emit and fail named emit with `unsupported-snapshot-session`; do not write `transcript.log` as a snapshot transcript. Thread the derived session id into `write_snapshot_generation_atomic` and write it unchanged to `manifest.json`.
  - Tests/checks: Update existing snapshot emit/redactor tests to create a staged native `*.jsonl` session file, assert `transcript_path` uses the layout extension, assert `session_id` equals the native file stem, and add coverage that missing staged transcripts skip auto-emit but fail named emit; run `cargo test -p rhei-cli --bin rhei snapshot`.

- Q-poll-self-loop-emits: Polling states write auto and named snapshots for every poll attempt instead of only when the poll exits the state.
  - Files: `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`, `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Approach: Move snapshot emission until after the outgoing transition has been selected for an agent invocation, but before that transition is applied. Pass the selected destination state into the emit hook and suppress both auto and named emit when the selected transition is a poll self-loop back to the current state. Preserve the existing final/gating/program suppression and completion classification.
  - Tests/checks: Add a polling-state test that runs a self-loop attempt and asserts no `_state` or named generation is written, then runs a terminal poll exit and asserts the expected generation is written once; run `cargo test -p rhei-cli --bin rhei snapshot poll`.

- Q-ancestor-select-state-order: `snapshot.inherit.from: ancestor` selects the nearest ancestor before applying `select.state`.
  - Files: `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`, `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Approach: Apply `inherit.select.state` while walking ancestors, so the chosen ancestor is the nearest ancestor with at least one orchestrator-produced snapshot matching both `inherit.name` and the optional selected emitting state. Keep target, visit, generation, completion, and compatibility filters after ancestor scope selection.
  - Tests/checks: Add an ancestor-resolution test where the nearest ancestor has the inherited name from a non-selected state and a farther ancestor has the selected state; assert the farther ancestor is selected instead of producing a missing-snapshot fallback; run `cargo test -p rhei-cli --bin rhei snapshot_inherit`.

## Rejected / Deferred

None.
