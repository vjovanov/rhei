# Point Decision - Resolve Panta defaults in state-machine resolution

- Chosen solution: Use the hybrid two-phase resolver: first bind the effective
  declaration name from rhei-local `**States:**`, then inherited
  `index.panta.md` default, then no declaration. If `--state-machine <path>` is
  supplied, it loads that file; when an effective declaration exists, the file's
  `name` must match it, and when no effective declaration exists, the file's
  `name` becomes the active name for that invocation. Without an override,
  rhei-local declarations resolve from the rhei sibling or Directory Workspace
  `states.yaml`; inherited Panta declarations resolve only from
  `<project>/states.yaml`; omitted effective declarations use the compiled
  built-in `rhei` machine and ignore discovered files.
- Why chosen: This is stronger than pure override-wins because it matches the
  existing plan-language and CLI behavior, preserves authored rhei/Panta policy
  as the source of the effective state-machine name, and still lets operators
  redirect file location or provide a machine for an otherwise omitted
  declaration. It also incorporates the points both discussions converged on:
  Panta inheritance is a default, not a merge; inherited defaults are anchored
  at the project root; validation and execution can share one resolver with
  source metadata; and invalid selected files fail closed.
- Alternatives considered: S-001's unconditional override-first order is
  simpler and gives operators maximum power, but it can silently mask authored
  or inherited policy and conflicts with current name-matching behavior. S-002's
  original file-only override for every case was too strict when no effective
  declaration exists, because then there is no authored name to preserve; in
  that omitted case, the override file should select the invocation's active
  name. Allowing child rheis to shadow a Panta default with a local
  `states.yaml` without redeclaring `**States:**` was rejected because it makes
  inheritance search-based instead of a deterministic project-root default.
- Remaining uncertainty: The final implementation should expose clear source
  metadata in diagnostics: override path, rhei-local declaration,
  `index.panta.md` inherited declaration, or built-in fallback. If future Panta
  designs add scoped defaults, this resolver will need an additional inherited
  source-selection rule before file lookup.
- Effect on final solution: The final answer must define one deterministic,
  non-merged resolver usable by validation and execution. A declared or
  inherited literal `rhei` may use a matching auto-discovered file from its
  source-specific lookup root, otherwise it falls back to the built-in `rhei`;
  omitted `**States:**` after Panta inheritance always means built-in fallback.
  A non-`rhei` declared or inherited name with no matching file is a validation
  error and never falls through, and an override file that does not match an
  effective authored or inherited name is also a validation error.
