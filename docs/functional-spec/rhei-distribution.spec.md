# FS-rhei-distribution: Rhei distribution and release process

Rhei releases ship the command-line binary, Rust crates, and release notes in a
repeatable process so users can install the same version from crates.io or a
GitHub release artifact. The release process must keep published package
versions, binary names, and release notes aligned with the workspace version.
§GOAL-rhei-outcomes

## 1. Release Targets

Each release publishes exactly two crates.io packages when crate publishing is
enabled, in this order:

1. `rhei-plan` — the plan model, parser, and workspace primitives, for
   callers that want to read Rhei plans without the CLI
2. `rhei-cli` — the tool itself, which depends on `rhei-plan`

Only packages with an audience outside this repository are published. Cargo
requires every dependency of a published package to be published too, so a
crate that exists purely to divide the CLI's own source would have to be
released, named, and version-locked forever for no one's benefit. Subsystems
without an external audience therefore live as modules inside `rhei-cli`
rather than as separate packages, and a new workspace crate is a decision to
add a permanent public package, not a way to organize files.

`rhei-agent-core` is deliberately unpublished while it remains a re-export of
`rhei-plan` with no callers; it becomes a release target when it has an
API of its own. The N-API crate stays unpublished for the same reason.

The crate name `rhei` belongs to an unrelated project on crates.io, so the CLI
publishes as `rhei-cli`; the crate name is not the command name.

`rhei-cli` installs two identical binaries, `rhei` and its short alias `rh`, so
both are on `PATH` after `cargo install rhei-cli`. GitHub release artifacts
package both names with `README.md` and a SHA-256 checksum; on platforms with
symbolic links the archive stores `rh` as a link to `rhei` rather than a second
copy. Public language API packages use the package name `rhei-api` on npm and
PyPI; native N-API support is an implementation detail and is not a crates.io
release target.

Both binaries are one line of hand-off, and the CLI does its work on a thread
whose stack it sizes itself rather than on the one the platform hands `main`.
The platforms disagree about that stack by a factor of eight — a Windows main
thread reserves 1 MiB where Linux and macOS give 8 — and the smaller of the two
is not enough for this CLI: on Windows every invocation overflowed it, `rhei`
with no arguments included. A stack asked for in code travels with the binary,
which a build-time linker setting does not: it is not read when someone
installs the published crate, which is how the binary reaches the machine this
concerns.

## 2. Version Source

The workspace package version in `Cargo.toml` is the release version. Internal
path dependencies that publish to crates.io must use the same exact version as
the workspace. Package wrappers under `packages/` also carry the same release
version so source-built npm and PyPI packages can install the matching
`rhei-cli` crate.

## 3. Release Modes

Releases can be started from a `vX.Y.Z` tag or manually from the release
workflow. Manual publishing creates or reuses the matching tag when crate
publishing or GitHub release creation is enabled. Dry runs can execute the
release build without publishing crates or creating a GitHub release.

## 4. PGO Binary Builds

Distributed GitHub release binaries are built with profile-guided optimization.
The PGO training run exercises the local repository and example plans through
the everyday CLI surfaces agents and contributors use most: version reporting,
validation, listing, rendering, state-machine inspection, template discovery,
and read-only next-task selection.

Source installs such as `cargo install rhei-cli --locked` use Cargo's ordinary
release profile instead of PGO. PGO is a packaging optimization, not a behavior
contract.

Release jobs build with a newer toolchain than the `rust-version` the crates
declare, because PGO instrumentation does not link on aarch64 Linux under the
MSRV compiler. `rust-version` states what a consumer needs in order to build
the published crates and is unaffected by the compiler that produced the
release binaries; the test matrix stays on the MSRV toolchain so the promise
keeps being checked. The release toolchain is pinned explicitly rather than
tracking stable, so every distributed binary in a release comes from one
compiler version.

## 5. Release Notes

`docs/changelog.md` contains an `Unreleased` section and the latest inline
release section. Release automation promotes `Unreleased` into a numbered
release section, archives the previous inline section under `docs/changelog/`,
and extracts the inline section for GitHub release notes.

## 6. Local Gates

The local pre-commit configuration mirrors the CI checks that are cheap enough
to run before a commit, and the pre-push hook reruns the Rust test suite. The
release PGO build is intentionally excluded from local commit hooks.

## 7. Supported Platforms

Rhei is a cross-platform tool, not a Unix tool with ports. The supported
platforms are the ones a release ships binaries for (§4): Linux, macOS, and
Windows, on `x86_64` and `aarch64`. Support means one thing on all of them:
§GOAL-rhei-outcomes

- **Parity.** Every user-visible behaviour — every command, prompt section,
  artifact, lock, and error — behaves the same on every supported platform.
  Where the platform genuinely differs (symbolic links, path spelling, process
  signals, file locking), the specification of that behaviour says so at the
  point where it differs, as §FS-rhei-snapshots.7 does for the `current`
  pointer; an undeclared difference is a defect on the platform that differs,
  never a limitation of it.
- **Tested, not assumed.** The development CI runs the full test suite on all
  three platforms (§AR-ci-release.1). A test that runs on a subset of platforms
  carries, at the gate, the platform-specific semantics it exercises and why
  no portable form exists; a test is never gated because porting it is work.
- **No Unix-only fixtures.** Test fixtures that stand in for agents, programs,
  callbacks, and redactors are written in a form every supported platform
  runs.
- **Paths are data.** Code never builds a path from `/`-joined strings, never
  compares two spellings of one location as strings, and treats a rooted or
  prefixed path as outside the workspace on every platform.

New work is held to this from the start: a feature is not complete until its
tests pass on all three platforms.
