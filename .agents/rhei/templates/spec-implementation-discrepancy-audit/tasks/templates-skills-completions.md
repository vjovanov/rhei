### Task templates-skills-completions: Audit templates, skills, completions, and generated workflows
**State:** scope-spec

Audit generated and assistant-facing workflow surfaces.

Spec scope:

- `{{spec_root}}/rhei-templates.spec.md`
- `{{spec_root}}/rhei-install-skills.spec.md`
- `{{spec_root}}/rhei-completions.spec.md`
- `{{spec_root}}/rhei-state-machine-writer.spec.md`
- relevant workflow examples in `{{spec_root}}/rhei-usage.spec.md`

Implementation roots:

{%- for root in implementation_roots %}
- `{{ root }}`
{%- endfor %}

Focus on template manifests, rendered plan validity, bundled `states.yaml`
lookup, generated task structure, parent/prior safety, skill installation paths,
completion shell behavior, and whether assistant-facing skills accurately encode
the current spec and validator constraints.
