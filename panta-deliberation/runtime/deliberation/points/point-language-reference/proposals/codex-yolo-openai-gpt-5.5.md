# Proposal - codex[yolo]:openai:gpt-5.5

- Recommendation: Keep one canonical language-reference entry point, and make
  Panta a first-class part of its "plan and project markdown" surface. The
  reference should enumerate `index.panta.md`, `rheis/` entries, and optional
  `inbox/` task files, then route readers to `§FS-rhei-plan-language.1.5` for
  the normative project layout and grammar, `§FS-rhei-panta` for user-facing
  behavior, and `§AR-rhei-panta` for load, id, execution-root, and state-binding
  mechanics. Do not create a separate Panta language reference and do not copy
  the full Panta grammar into the canonical reference.
- Reasons: This gives users one discoverable entry point while keeping
  normative detail with the specs that can validate it. `index.panta.md` is
  manifest syntax, `rheis/` entries are normal rhei syntax discovered at project
  scope, and `inbox/` files are normal workspace task-file syntax under a
  synthetic rhei; documenting them as one plan/project markdown surface matches
  the actual parser and avoids a second, inconsistent model. It also keeps ids,
  cross-rhei dependencies, Panta defaults, state-policy lookup, execution roots,
  and command behavior tied to the same project graph model.
- Tradeoffs: The canonical reference remains a map, not a self-contained manual;
  readers must follow links for exact grammar and behavior. The split requires
  update discipline: any future Panta syntax change must update the language
  reference and the owning spec together. If Panta authoring grows much larger,
  the reference may need a deeper sub-table of contents, but still should not
  duplicate the normative rules.
- Assumptions: Panta authoring is an extension of the plan/project markdown
  language surface, not a separate language. The inbox remains a reserved
  synthetic level-1 rhei whose files parse as ordinary workspace task files.
  State-machine resolution remains per rhei, with rhei-local declarations,
  inherited `index.panta.md` defaults, and fallback handled by the shared lookup
  rule.
- Rejection criteria: Do not use this proposal if the team wants the canonical
  language reference to be the complete normative grammar instead of a routing
  page, if Panta gets a separate parser/language independent of plan markdown,
  or if the final model stops treating `rheis/` entries and `inbox/` files as
  ordinary rhei/workspace syntax loaded at project scope.
