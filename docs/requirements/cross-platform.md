# REQ-cross-platform: One tool on Linux, macOS, and Windows

Rhei is a cross-platform tool, not a Unix tool with ports. This requirement
holds for every feature from the moment it is specified: a feature is not
complete until its tests pass on every supported platform. §GOAL-rhei-outcomes

## 1. Supported Platforms

The supported platforms are the ones a release ships binaries for
(§FS-rhei-distribution.4): Linux, macOS, and Windows, on `x86_64` and
`aarch64`. Support means the same thing on all of them — the rules below.

## 2. Parity

Every user-visible behaviour — every command, prompt section, artifact, lock,
and error — behaves the same on every supported platform. Where the platform
genuinely differs (symbolic links, path spelling, process signals, file
locking), the specification of that behaviour says so at the point where it
differs, as §FS-rhei-snapshots.7 does for the snapshot `current` pointer. An
undeclared difference is a defect on the platform that differs, never a
limitation of it.

## 3. Tested, Not Assumed

The development CI runs the full test suite on all three platforms
(§AR-ci-release.1). A test that runs on a subset of platforms carries, at the
gate, the platform-specific semantics it exercises and why no portable form
exists; a test is never gated because porting it is work.

## 4. Portable Fixtures

Test fixtures that stand in for agents, programs, callbacks, and redactors are
written in a form every supported platform runs — never a shell script.

## 5. Paths Are Data

Code never builds a path from `/`-joined strings, never compares two spellings
of one location as strings, and treats a rooted or prefixed path as outside
the workspace on every platform.
