# Implementation Notes: impl-rhei-states

Spec: `docs/functional-spec/rhei-states.spec.md` (`§FS-rhei-states`)

## Coverage

- Schema additions / validation rules: implemented in `crates/rhei-validator/src/lib.rs`.
  - Version 3 state machines now require `profiles` and `node_policy`.
  - `node_policy.overrides[].match` now uses the spec shape `{ type?, level? }` with closed-field validation.
  - Profile validation now enforces a final state in every `allowed` set and reachability from every non-final allowed state to an allowed final state.
  - Plan-dependent node-policy validation checks `by_type` and override selectors against `structure.nodeKinds` and `structure.maxLevels` during plan validation.
  - Authored node states continue to be checked against the node's resolved profile `allowed` set.

- Profiles / Node Policy: implemented in `crates/rhei-validator/src/lib.rs` and `crates/rhei-cli/src/main.rs`.
  - Added `StateMachine::profile_for_node(kind, level)` using the spec resolution order: first matching override, then `by_type`, then `default`.
  - `rhei reset` now resolves each task node's profile and resets that node to the profile's `initial`, rather than using one global initial state.
  - Legacy v1 machines without profiles still fall back to the older single `initial: true` behavior so existing v1 fixtures remain loadable.

- States / Transitions default machine: implemented in `crates/rhei-validator/src/default-states.yaml`.
  - The built-in default machine now matches `docs/functional-spec/states.yaml` exactly, including version `3.0`, `profiles`, and `node_policy`.
  - The writer-skill mirror `skills/rhei-plan-writer/references/default-states.md` already described the same profile-based default.

- Polling states, artifact contracts, template variables, agent/program fields, MCP servers/skills, target/model validation, and snapshot interactions: already implemented across `crates/rhei-validator/src/lib.rs` and `crates/rhei-cli/src/main.rs`; this pass kept those surfaces intact and added regression coverage around the missing profile/node-policy pieces.

## Tests Added Or Updated

- `crates/rhei-validator/src/lib.rs`
  - v3 machines without `profiles`/`node_policy` are rejected.
  - type/level node-policy overrides resolve before `by_type`.
  - profiles without a path to an allowed final state are rejected.
  - node-policy selectors are validated against plan structure.

- `crates/rhei-cli/src/main.rs`
  - reset rewriting still supports legacy single-initial machines.
  - reset rewriting uses resolved profile initials per node when profiles are present.

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings -W clippy::all`
- `cargo build --workspace --all-targets`
- `cargo test -p rhei-cli-validator --lib`
- `cargo test -p rhei-cli --bin rhei`
- `cargo test -p rhei-cli --test integration_markdown_plans reset -- --nocapture`

`cargo test --workspace --all-targets --no-fail-fast` was also run. It completed most suites but failed two existing e2e cases outside this task's edited surface:

- `e2e::run_tests::changeset_review_human_review_state_is_gating_in_shipped_workflows`: `examples/changeset-review-example/states.yaml` has no `human-review` state in the current workspace.
- `e2e::next_tests::next_does_not_auto_transition_runnable_initial_states`: the test state machine names model `codex`, but current settings validation reports no `settings.models.codex` profile.

## Deferrals

- None for the assigned v3 states schema. Legacy v1 `initial: true` machines remain supported as a compatibility path; v3 machines use the profile/node-policy schema required by `§FS-rhei-states`.

Ready for `completeness-review`.
