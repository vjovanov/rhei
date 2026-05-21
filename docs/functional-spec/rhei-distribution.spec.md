# FS-rhei-distribution: Rhei distribution and release process

Rhei releases ship the command-line binary, Rust crates, and release notes in a
repeatable process so users can install the same version from crates.io or a
GitHub release artifact. The release process must keep published package
versions, binary names, and release notes aligned with the workspace version.
§GOAL-rhei-outcomes

## 1. Release Targets

Each release publishes these crates.io packages when crate publishing is
enabled:

- `rhei-plan-core`
- `rhei-cli-tui`
- `rhei-agent-core`
- `rhei-cli-output`
- `rhei-cli-validator`
- `rhei-api-napi`
- `rhei-cli`

The installed CLI binary is named `rhei`. GitHub release artifacts package that
binary with `README.md` and a SHA-256 checksum.

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

## 5. Release Notes

`docs/changelog.md` contains an `Unreleased` section and the latest inline
release section. Release automation promotes `Unreleased` into a numbered
release section, archives the previous inline section under `docs/changelog/`,
and extracts the inline section for GitHub release notes.

## 6. Local Gates

The local pre-commit configuration mirrors the CI checks that are cheap enough
to run before a commit, and the pre-push hook reruns the Rust test suite. The
release PGO build is intentionally excluded from local commit hooks.
