# Point Decision - Resolve Panta syntax in the canonical language reference

- Chosen solution: Use S-001 with a compact, structured file-kind map in the
  canonical language reference. The map should replace the dense
  plan/project-markdown bullet with scoped rows for the user-authored Panta
  surface and the adjacent rhei/workspace shapes needed to understand it:
  `index.panta.md`, Panta `rheis/` entries, `*.rhei.md`,
  `index.rhei.md` plus workspace `tasks/**/*.md`, optional `inbox/` task files,
  and bare-rhei loading where relevant. Each row should give only a short role
  and owner citations. `states.yaml` remains under the separate state-machine
  language surface, while Panta rows link to the state-resolution/defaulting
  rules when needed. Use functional specs as the primary user-facing owners
  (`§FS-rhei-plan-language.1.5`, `§FS-rhei-plan-language.1.1`,
  `§FS-rhei-plan-language.1.2`, `§FS-rhei-plan-language.1.3`,
  `§FS-rhei-panta`, and `§FS-rhei-panta.6`) and include `§AR-rhei-panta` as the
  mechanics destination for load order, id namespacing, execution roots, and
  state-machine binding.
- Why chosen: The current `§FS-rhei-language-reference` explicitly exists to
  answer what files and syntax make up a valid Rhei workflow, but its single
  plan/project-markdown bullet already compresses too many file kinds into one
  line. A small file-kind map makes the required Panta files discoverable by
  name, preserves one canonical entry point, and avoids duplicating the grammar
  because every row routes to its owning spec. Including adjacent rhei and
  workspace shapes is stronger than a Panta-only list because `rheis/` entries
  and `inbox/` files are meaningful only as ordinary rhei/workspace syntax
  loaded at project scope. Keeping `states.yaml` out of that map preserves the
  existing separation between plan/project markdown and the state-machine
  surface.
- Alternatives considered: A full broad map including root `states.yaml` and
  every nearby authored file kind was rejected because it blurs the current
  language-surface boundary and risks making plan/project markdown own
  state-machine syntax. A lighter prose or per-line Panta-only enumeration was
  viable and would satisfy the minimum requirement, but it is weaker for
  discoverability: readers still have to infer what shapes a `rheis/` entry may
  take and how inbox task files relate to workspace task syntax. A separate
  Panta language reference was rejected because both proposals agree Panta is an
  extension of the plan/project markdown surface, not an independent language or
  second grammar source.
- Remaining uncertainty: None for this point. The exact markdown rendering may
  be a table or an equivalent scoped list, but it must behave as a file-kind map
  with role text and owner citations rather than as copied normative grammar.
- Effect on final solution: The final answer must update the canonical language
  reference as the single discoverable entry point for Panta authoring syntax.
  It must explicitly name `index.panta.md`, Panta `rheis/` entries, and optional
  `inbox/` task files; show how those entries relate to ordinary rhei and
  workspace markdown; route grammar and user-facing behavior to functional
  specs; route load/id/execution-root/state-binding mechanics to
  `§AR-rhei-panta`; keep `states.yaml` in the state-machine surface; and add an
  ownership rule that any addition, removal, or rename of a user-authored
  project/rhei file kind or directory updates the language-reference map in the
  same change as the owning spec edit, while broader authoring-syntax changes
  continue to update the canonical reference and owner spec together.
