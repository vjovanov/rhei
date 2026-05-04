### Task templates-skills-completions: Audit templates, skills, completions, and generated workflows
**State:** scope-spec

Audit generated and assistant-facing workflow surfaces.

Spec scope:

- `docs/specs/rhei-templates.spec.md`
- `docs/specs/rhei-install-skills.spec.md`
- `docs/specs/rhei-completions.spec.md`
- `docs/specs/rhei-state-machine-writer.spec.md`
- relevant workflow examples in `docs/specs/rhei-usage.spec.md`

Implementation roots:
- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Focus on template manifests, rendered plan validity, bundled `states.yaml`
lookup, generated task structure, parent/prior safety, skill installation paths,
completion shell behavior, and whether assistant-facing skills accurately encode
the current spec and validator constraints.