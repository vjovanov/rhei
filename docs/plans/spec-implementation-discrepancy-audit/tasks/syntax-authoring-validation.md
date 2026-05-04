### Task syntax-authoring-validation: Audit plan syntax, authoring, parsing, and validation
**State:** scope-spec

Audit the core plan language and validation contract.

Spec scope:

- `docs/rhei.spec.md`
- `docs/specs/rhei-authoring.spec.md`

Implementation scope:

- `crates/rhei-core/src/`
- `crates/rhei-validator/src/`
- parser, lexer, AST, structure, dependency, state, metadata, and workspace validation tests under `crates/*/tests/`
- CLI validation entry points in `crates/rhei-cli/src/main.rs`

Focus on markdown grammar, directory workspace semantics, task hierarchy rules,
metadata fields, `**States:**` lookup, prior dependency semantics, terminal child
coherence, artifact contract validation, and diagnostics promised by the specs.
