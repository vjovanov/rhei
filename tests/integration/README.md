# integration

Integration tests prove the How: that the parts fit as designed. A test
belongs here when its subject spans more than one part — the CLI driving the
core plan model, a workspace of several task files, callbacks crossing the
process boundary — rather than one module in isolation. This is the home of
the non-citable `integration` kind (`grund.toml`, `[[kinds]] kind =
"integration"`); `[citations.integration]` says the home should cite `AR`,
since an integration test's subject is usually a structural claim rather than
a user-visible one. Black-box proof of a spec point belongs in `../e2e/`
instead; a claim about one module is a unit test beside the code it tests.

## Layout

This directory is also the workspace member `rhei-integration-tests` (never
published). `integration_markdown_plans.rs` is the crate's one `[[test]]`
harness: it `include!`s every file under `integration_markdown_plans/` into
one flat module, grouped by command or behavior area — parsing and
validation, transitions, callbacks, `rhei run`, `reset`, and workspace
discovery and validation. `../support/` holds the process and fixture helpers
this harness shares with `rhei-e2e-tests`, pulled in with `include!` rather
than `mod` because this harness is itself one flat module.

## Rust

`cargo test --workspace --all-targets` builds and runs these; `cargo test -p
rhei-integration-tests` runs them alone and builds the `rhei` binary on demand
(`../support/binaries.rs`) when the workspace build has not already produced
it.

`crates/rhei-core/tests/` (`lexer_smoke.rs`, `lexer_edge_cases.rs`) stays
where it is: a claim about the parser alone is a unit test, not an integration
one, even though `fixtures.rs` there is shared with this harness.
