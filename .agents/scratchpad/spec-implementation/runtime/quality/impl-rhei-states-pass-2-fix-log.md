# Quality Fix Log pass 2

## Accepted Fixes Applied

- Q-profile-allowed-not-enforced-on-transition
  - Files edited:
    - `crates/rhei-cli/src/cli/system_transition_execution.rs`
    - `crates/rhei-cli/src/cli/ready_transition.rs`
    - `crates/rhei-cli/tests/integration_markdown_plans/transitions_failures_completion.rs`
    - `crates/rhei-cli/tests/integration_markdown_plans/callbacks_redirect_context.rs`
  - Notes:
    - Added a per-task resolved-profile guard before committing explicit transitions.
    - Filtered automatic transition selection so destinations outside the task's resolved profile `allowed` set are skipped.
    - Rechecked callback `nextState` redirects before accepting the redirected destination.
    - The requested test path `crates/rhei-cli/tests/integration_markdown_plans/states.rs` does not exist in this checkout; regression coverage was added to the existing included integration test files.

## Checks

- `cargo fmt --all` - passed
- `cargo test -p rhei-cli --test integration_markdown_plans states` - failed once because two assertions depended on single-line diagnostic formatting; assertions were narrowed to the intended content.
- `cargo test -p rhei-cli --test integration_markdown_plans states` - passed

## Left Unfixed

- None.

## Next State

- Pass 2 of 2 is complete, so the task should advance from `quality-fix` to `completed`.
