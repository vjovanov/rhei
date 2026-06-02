### Task point-state-machine-resolution: Resolve Panta defaults in state-machine resolution
**State:** completed
**Prior:** Task split

Resolve how project defaults participate in state-machine lookup for rheis
inside a Panta project.

Source evidence:

> If a rhei inside a project omits `**States:**`, `index.panta.md` may supply
> the project default.

> The state-machine resolution rules need one non-conflicting lookup order for
> explicit overrides, rhei-local declarations, inherited Panta defaults, and
> built-in fallback.

Question:

What single lookup order should resolve a rhei's state machine when explicit
overrides, rhei-local declarations, inherited Panta defaults, and built-in
fallback may all apply?

Constraints:

- Omitted `**States:**` on a rhei may inherit a project default from
  `index.panta.md`.
- Explicit overrides, rhei-local declarations, inherited Panta defaults, and
  built-in fallback must each have defined precedence.
- The lookup order must be non-conflicting and usable by validation and
  execution.
