# e2e

Black-box, user-scenario proof of the spec: every test here runs the real
`rhei` binary as a subprocess and asserts on what it prints and writes, the
way a user or an agent would invoke it — never on the crates behind it, across
the whole command surface and the roles and patterns it serves. [§FS-rhei-usage](../../docs/functional-spec/rhei-usage.spec.md#fs-rhei-usage-how-rhei-is-used)

This directory is the home of the non-citable `e2e` kind (`grund.toml`,
`[[kinds]] kind = "e2e"`): a scenario is exercised by being run, never cited,
so no test here declares an ID. `[citations.e2e]` says the home must cite `FS`
and should not cite `AR` — a scenario proves the What as a user sees it, and
one that reads the design is not black-box. Every top-level test file carries
the `§FS-…` citation for the behavior it proves, at the top of the file or on
the specific test that needs the narrower section.

## Layout

This directory is also the workspace member `rhei-e2e-tests` (never
published). `mod.rs` is the crate's one `[[test]]` harness root — the module
list `cargo test --workspace` and `cargo test -p rhei-e2e-tests` both build —
and every sibling `*_tests.rs` file is a `mod` of it, one file per command or
behavior area, matching the split `AR-source-file-size` asks for. Shared
fixture and process helpers live in `../support/`, pulled in with `#[path]`
because they are reached from both this crate and `rhei-integration-tests`.

`fixtures/` holds multi-file workspace scenarios (`living-review-loop/`,
`script-agent-team/`) that tests copy into a temporary directory before
running `rhei` against them. It is test-input data, not a citable or scanned
document, so `[scan] exclude` keeps it out of the host scan the same way
`templates/` is excluded.

## Rust

`cargo test --workspace --all-targets` builds and runs these; `cargo test -p
rhei-e2e-tests` runs them alone and builds the `rhei` binary on demand
(`../support/binaries.rs`) when the workspace build has not already produced
it.

Unit tests live with the code they test (`crates/*/src/`); a claim about one
module stays there. A test belongs here once it drives the built binary as a
subprocess rather than calling the crates directly — that boundary is what
`../integration/` and `crates/rhei-core/tests/` sit on the other side of.
