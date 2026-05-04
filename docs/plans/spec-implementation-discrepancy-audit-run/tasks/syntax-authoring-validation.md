### Task syntax-authoring-validation: Audit plan syntax, authoring, parsing, and validation
**State:** human-decision

Audit the core plan language and validation contract for Rhei.

Spec scope:

- `docs/rhei.spec.md`
- `docs/specs/rhei-authoring.spec.md`

Implementation roots:
- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Focus on markdown grammar, directory workspace semantics, task hierarchy rules,
metadata fields, `**States:**` lookup, prior dependency semantics, terminal child
coherence, artifact contract validation, and diagnostics promised by the specs.