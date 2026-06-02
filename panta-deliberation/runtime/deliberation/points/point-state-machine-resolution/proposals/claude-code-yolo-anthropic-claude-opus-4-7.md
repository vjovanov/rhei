# Proposal - claude-code[yolo]:anthropic:claude-opus-4-7

- Recommendation: Resolve in two strictly ordered phases — first the effective **name**, then the **file** keyed by which layer supplied that name.

  Phase A — Effective `**States:**` name (first match wins):
    1. Rhei-local `**States:**` declaration in the rhei's own header.
    2. Inherited declaration from `index.panta.md` (Panta default), only when the rhei sits inside a Panta project and omits `**States:**`.
    3. Built-in fallback name `rhei`.

  Phase B — File resolution, dispatched by Phase A's source:
    1. If `--state-machine <path>` is supplied, it loads that file unconditionally; the loaded file's `name` must equal the name from Phase A. If Phase A landed on the built-in fallback (step A.3), the loaded file's name becomes the effective name.
    2. Else if Phase A.1 (rhei-local): load `states.yaml` sibling to the rhei (single-file plan) or `<workspace>/states.yaml` (Directory Workspace).
    3. Else if Phase A.2 (Panta-inherited): load `<project>/states.yaml`.
    4. Else (Phase A.3 fallback): use the built-in `rhei` machine; sibling/workspace/project `states.yaml` are ignored.

  Uniform error rules: a non-`rhei` declared name with no matching file is a validation error and never falls back; a `--state-machine` whose `name` disagrees with the Phase-A name is a validation error.

- Reasons: Splitting **name resolution** from **file resolution** removes the only real source of conflict — two layers competing to both *name* and *locate* the machine. With one ordered Phase A, every rhei has exactly one effective name; with one ordered Phase B keyed by *where the name came from*, the file lookup root is determined, never searched. This is implementable as a pure function `(rhei, panta, cli_override) → (name, file_or_builtin)`, which validation and execution can call identically. Treating `--state-machine` as a file-level override that must be name-consistent (rather than as a name-level override) preserves the "rhei declares its own state machine" invariant while still allowing operators to point at an alternate file (e.g. a fork) when the names agree.

- Tradeoffs: The system must track each rhei's *source layer* (own vs. panta-inherited vs. defaulted), not just the resulting name — small bookkeeping cost during plan load, but it must be exposed to validation so error messages can say "inherited from `index.panta.md`". A Panta-inherited declaration looks for `states.yaml` only at the project root, so a rhei cannot shadow the project default with a sibling file without also redeclaring `**States:**` locally — explicit, but slightly more typing. `--state-machine` cannot rename the machine; an operator who wants to switch *names* must edit the declaration or rely on Phase A.3 omission. Default `**States: rhei**` declared by a rhei (Phase A.1, name = `rhei`) does allow an auto-discovered sibling `states.yaml` named `rhei` to override the built-in, mirroring the existing spec point §FS-rhei-plan-language.1.3 — this is a small asymmetry with Phase A.3 but keeps backward compatibility with existing single-file plans.

- Assumptions: `index.panta.md` declares at most one project-wide `**States:**` value (no per-subdirectory Panta defaults). The built-in `rhei` machine is compiled into the CLI and is the only built-in. State-machine `name` is a stable property of the YAML file and matching is exact (no aliases). Every rhei's location is resolvable so "sibling" / "workspace" / "project root" are unambiguous. Validation and execution share a single resolver implementation so the order cannot drift between commands.

- Rejection criteria: Do not use this proposal if Panta-inherited declarations must allow rhei-local file overrides (a rhei-sibling `states.yaml` shadowing `<project>/states.yaml`) without redeclaring `**States:**` — that requires Phase B to search rhei-local *then* project root for inherited names, reintroducing search ambiguity. Do not use if `--state-machine` should be allowed to silently change the effective name (rename rather than validate). Do not use if the design must support multiple Panta default scopes (e.g. subgroup-level defaults under one project), because Phase A.2 assumes a single inherited source. Do not use if Phase A.1 with the literal name `rhei` should *not* trigger sibling-file lookup — the asymmetry with Phase A.3 would need to be removed by forbidding auto-discovery for the name `rhei` entirely.
