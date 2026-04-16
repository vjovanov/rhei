# Agent Workflow Notes

## Specification

See the [Rhei Plan Language Specification](docs/plan-language-spec.md).

## CI Verification Commands

Run these commands from the repository root to mirror CI checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets
cargo test --workspace --all-targets --no-fail-fast
```
