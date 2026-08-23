# AR-ci-release: CI and release automation mirror local gates

Rhei uses GitHub Actions as the remote authority for formatting, linting,
build, test, grounding, pre-commit, and release checks. The workflow layout
keeps normal pull-request feedback fast while moving slower packaging work to
pre-release and release workflows. §FS-rhei-distribution

## 1. Development CI

The `CI` workflow runs on pushes and pull requests as two jobs that run in
parallel, so pull-request feedback takes as long as the slowest test platform
and no longer.

**`test`** runs on Linux, macOS, and Windows. Each platform installs the pinned
Rust toolchain from `rust-toolchain.toml`, restores the cargo registry and
`target` cache, and executes the Rust formatting, lint, and build gates. Linux
and macOS also run the full Rust test suite; Windows remains a compile and lint
portability gate because several CLI fixtures intentionally exercise Unix shell
and file-lock semantics (running the suite there is tracked separately):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked --no-fail-fast
```

**`lint`** runs on Linux only. It runs `grund config validate` and
`grund check .`, then the repository `.pre-commit-config.yaml` against all
files with the cargo hooks skipped — `test` has just run them on three
platforms, and running the suite a second time on one of them bought nothing —
so the remaining hook contract (fissile, lychee, attribution boilerplate) is
enforced remotely, and on pull requests the changelog entry check. The gate
binaries (`grund`, `lychee`, `fissile`) are installed from source only on a
cache miss: they live under a root of their own keyed by their pinned versions,
so a version bump rebuilds exactly that tool and nothing else.

Both jobs stay inside the one `CI` workflow because the release helpers (§3)
look the green run up by workflow name.

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
