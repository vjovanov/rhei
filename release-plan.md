# Rhei Release Plan

This plan is for the `0.1.0-alpha.1` release.

## Naming

The product and command are called `rhei`.

| Surface | Package | User-facing name |
|---------|---------|------------------|
| crates.io CLI | `rhei-cli` | `rhei` binary |
| crates.io Rust API | `rhei-api` | `rhei_core` crate import |
| crates.io support crates | `rhei-cli-output`, `rhei-cli-validator`, `rhei-cli-tui`, `rhei-api-napi` | `rhei_output`, `rhei_validator`, `rhei_tui`, `rhei_napi` crate imports |
| npm CLI | `rhei` | `rhei` command |
| npm API | `rhei-api` | `require("rhei-api")` |
| PyPI CLI | `rhei-cli` | `rhei` command |
| PyPI API | `rhei-api` | `import rhei_api` |

PyPI cannot use the package name `rhei` because it is already taken by an
unrelated project. crates.io cannot use `rhei`, `rhei-core`, or `rhei-tui`
because those names are already taken.

## Legend

- **Human required**: needs account ownership, credentials, irreversible publish
  confirmation, or release judgment.
- **Scriptable**: can be automated once credentials and final release inputs are
  available.

## Phase 0: Create Accounts and Claim Access

Status: **Human required**

Do this before attempting any release commands.

1. Create a crates.io account.
   - Sign in with GitHub on crates.io.
   - Create a crates.io API token.
   - Log in locally:

```bash
cargo login <crates-token>
```

2. Create an npm account.
   - Create the account on npmjs.com.
   - Enable 2FA if prompted.
   - Log in locally:

```bash
npm login
```

3. Create a PyPI account.
   - Create the account on pypi.org.
   - Enable 2FA.
   - Create a PyPI API token.
   - Install local packaging tools:

```bash
python3 -m pip install --user build twine
```

4. Optional but recommended: create a TestPyPI account.
   - Create the account on test.pypi.org.
   - Use it for a practice upload before the real PyPI release.

5. Verify GitHub release access.

```bash
git remote -v
git push --dry-run origin HEAD
```

Package names are claimed at first publish. Create the accounts first, then
publish the alpha soon enough to reserve the intended names with real packages.

## Phase 1: Local Release Preparation

Status: **Scriptable**

Run from the repository root:

```bash
cargo test --workspace --no-fail-fast
cargo publish --dry-run -p rhei-api
cargo publish --dry-run -p rhei-cli-tui
```

## Final Preflight

Status: **Scriptable**

Run this immediately before publishing:

```bash
cargo test --workspace --no-fail-fast
cargo publish --dry-run -p rhei-api
cargo publish --dry-run -p rhei-cli-tui
```

The checked-in release script runs this preflight plus npm/PyPI packaging
checks:

```bash
scripts/release-all.sh
```

The default mode is a dry run and does not publish anything.

## Publish crates.io

Status: **Human required to initiate actual publish; scriptable command
sequence after credentials are configured.**

Publishing to crates.io is irreversible for package names and versions. Run the
dry-runs in a script if desired, but have a human explicitly start the real
publish sequence.

To publish all configured platforms after logins are set up:

```bash
scripts/release-all.sh --publish --push-git-tag
```

To also create the GitHub release with `gh`:

```bash
scripts/release-all.sh --publish --github-release
```

Publish base crates first:

```bash
cargo publish -p rhei-api
cargo publish -p rhei-cli-tui
```

Wait 1-2 minutes for crates.io index propagation.

Publish direct dependents:

```bash
cargo publish --dry-run -p rhei-cli-output
cargo publish -p rhei-cli-output

cargo publish --dry-run -p rhei-cli-validator
cargo publish -p rhei-cli-validator

cargo publish --dry-run -p rhei-api-napi
cargo publish -p rhei-api-napi
```

Wait 1-2 minutes for crates.io index propagation.

Publish the CLI crate:

```bash
cargo publish --dry-run -p rhei-cli
cargo publish -p rhei-cli
```

## Publish npm

Status: **Human required to initiate actual publish; scriptable command
sequence after credentials are configured.**

Before publishing, verify the npm CLI package name is the intended one:
`packages/npm/rhei-cli/package.json` should use `"name": "rhei"` if the install
story is `npm install -g rhei`.

Publish npm after crates.io `rhei-cli` is available, because the npm CLI package
installs the Rust CLI through Cargo.

```bash
cd packages/npm/rhei-cli
npm pack --dry-run
npm publish --access public
```

Then publish the npm API package:

```bash
cd ../rhei-api
npm pack --dry-run
npm publish --access public
```

## Publish PyPI

Status: **Human required to initiate actual upload; scriptable build/check/upload
sequence after credentials are configured.**

Publish PyPI after crates.io `rhei-cli` is available, because the Python CLI
package installs the Rust CLI through Cargo on first use.

```bash
cd packages/python/rhei-cli
python3 -m build
python3 -m twine check dist/*
python3 -m twine upload dist/*
```

Then publish the PyPI API package:

```bash
cd ../rhei-api
python3 -m build
python3 -m twine check dist/*
python3 -m twine upload dist/*
```

## Tag and GitHub Release

Status: **Human required for release notes/release creation; tag commands are
scriptable after final confirmation.**

Return to the repository root:

```bash
cd /home/vjovanov/c/rhei
git tag -a v0.1.0-alpha.1 -m "Rhei 0.1.0-alpha.1"
git push origin v0.1.0-alpha.1
```

Create a GitHub release from the `CHANGELOG.md` entry.

## Smoke Tests

Status: **Scriptable**

Run after publishing:

```bash
cargo install rhei-cli --locked --force
rhei version
```

```bash
npm install -g rhei
rhei version
```

```bash
python3 -m pip install rhei-cli
rhei version
```

Optional API smoke checks:

```bash
npm install rhei-api
node -e 'console.log(require("rhei-api").version())'
```

```bash
python3 -m pip install rhei-api
python3 -c 'import rhei_api; print(rhei_api.version())'
```

## Later

Status: **Human decision required; implementation can be scripted later.**

Do not block the alpha on these:

- Homebrew tap formula named `rhei`
- GHCR image, only if a service or CI runtime becomes useful
- Maven artifacts, only if a JVM API or plugin is added
