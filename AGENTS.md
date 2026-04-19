# Agent Workflow Notes

## Specification

- See the [Rhei Plan Language Specification](docs/rhei.spec.md).
All textual spec files must end with `.spec.<file-ending>`.
- [ADR (Architecture Decision Record)](docs/adr/adr.md)
- Follow progressive disclosre in the spec

## CI Verification Commands

Run these commands from the repository root to mirror CI checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets
cargo test --workspace --all-targets --no-fail-fast
```
