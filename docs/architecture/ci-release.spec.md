# AR-ci-release: CI and release automation mirror local gates

Rhei uses GitHub Actions as the remote authority for formatting, linting,
build, test, grounding, pre-commit, and release checks. The workflow layout
keeps normal pull-request feedback fast while moving slower packaging work to
pre-release and release workflows. §FS-rhei-distribution

## 1. Development CI

The `CI` workflow runs on pushes and pull requests across Linux, macOS, and
Windows. Each platform installs the pinned Rust toolchain from
`rust-toolchain.toml`, runs `grund config validate`, runs `grund check .`, then
executes the Rust formatting, lint, and build gates. Linux and macOS also run
the full Rust test suite; Windows remains a compile and lint portability gate
because several CLI fixtures intentionally exercise Unix shell and file-lock
semantics:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked --no-fail-fast
```

Linux also runs the repository `.pre-commit-config.yaml` against all files so
the local hook contract is enforced remotely.

## 2. Local Hooks

The pre-commit hooks run `grund`, formatting, clippy, build, tests, changelog
checks, link checks, and attribution boilerplate checks before a commit. The
pre-push hook reruns tests and checks that an open pull request has a matching
`docs/changelog.md` `Unreleased` entry.

## 3. Release Workflows

The release workflow verifies the requested version against the selected source
ref, checks package-name ownership or availability, builds PGO binaries for the
supported release platforms, publishes crates.io packages in dependency order
when requested, and creates or updates the GitHub release from the extracted
changelog notes.

Patch and minor release helper workflows follow the same model as the release
workflow: they require a green `CI` run on `main`, create a version bump commit,
dry-run the release workflow from the candidate branch, then fast-forward
`main` and dispatch the publishing release.

## 4. PGO Boundary

PGO is exercised by the manual pre-release workflow and the release workflow,
not by the normal development CI matrix. This keeps pull-request feedback tied
to correctness and API behavior while still verifying that packaged binaries
can be generated before a release. §FS-rhei-distribution.4
