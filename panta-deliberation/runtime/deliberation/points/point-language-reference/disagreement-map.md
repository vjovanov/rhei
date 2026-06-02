# Disagreement Map - Resolve Panta syntax in the canonical language reference

## Candidate Solutions

- S-001: Keep one canonical language-reference entry point, and make Panta a
  first-class part of its plan/project markdown surface. Enumerate
  `index.panta.md`, Panta `rheis/` entries, and optional `inbox/` task files,
  then route readers to the owning normative specs instead of duplicating the
  grammar.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7,
    codex[yolo]:openai:gpt-5.5
  - Reasons: Gives users one discoverable entry point, satisfies the requirement
    that Panta-authored files are documented as language surface, and avoids a
    second grammar source that could drift from validation and execution rules.

- S-002: Implement S-001 as a structured file-kind map organized by container
  scope: Project/Panta, Rhei, workspace tasks, inbox, and bare rhei. Each entry
  names the authored surface, gives a one-line role, and cites the single owning
  spec section.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7
  - Reasons: Makes every user-authored file kind discoverable by name, mirrors
    the Panta -> rhei -> task load hierarchy, and gives reviewers a concrete
    map to keep synchronized with future language changes.

- S-003: Implement S-001 as a lighter enumeration in the existing plan/project
  markdown section, with links to `§FS-rhei-plan-language.1.5`,
  `§FS-rhei-panta`, and `§AR-rhei-panta`; add a deeper table of contents only
  if Panta authoring grows larger.
  - Proposed by: codex[yolo]:openai:gpt-5.5
  - Reasons: Keeps the canonical reference concise as a routing page while
    still surfacing the required Panta files and tying ids, execution roots,
    state binding, defaults, and command behavior to the project graph model.

- S-004: Add an ownership/update rule requiring Panta or plan/project
  authoring-syntax changes to update the language reference and the owning spec
  together.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7,
    codex[yolo]:openai:gpt-5.5
  - Reasons: Both proposals identify update discipline as necessary to prevent
    the canonical entry point from going stale while normative detail remains in
    specialized specs.

## Agreements

- A-001: The canonical language reference should remain the single discoverable
  entry point for user-authored plan/project markdown syntax.
- A-002: `index.panta.md`, Panta `rheis/` entries, and optional `inbox/` task
  files must be explicitly named as user-authored language surface.
- A-003: The canonical reference should not become a self-contained copy of the
  Panta grammar or create a separate Panta language reference.
- A-004: Normative detail should remain with owning specs: project layout and
  grammar in `§FS-rhei-plan-language.1.5`, user-facing Panta behavior in
  `§FS-rhei-panta`, and implementation/load/id/state-binding mechanics where
  those are already owned.
- A-005: Panta authoring is an extension of the plan/project markdown surface,
  not an independent language.
- A-006: `rheis/` entries should be described as normal rhei syntax discovered
  at project scope, and `inbox/` files should be described as ordinary workspace
  task-file syntax loaded under the synthetic inbox rhei.
- A-007: Future changes to authored Panta or plan/project file kinds need an
  explicit update path so the canonical reference does not drift from the owning
  specs.

## Disagreements

- D-001: Whether the canonical language reference should expose Panta syntax
  through a structured file-kind map or through a lighter prose/list
  enumeration.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 proposes a scoped
    file-kind map with layer, authored surface, role, and owner columns;
    codex[yolo]:openai:gpt-5.5 proposes first-class enumeration and links, with
    a deeper sub-table of contents only if the surface grows.
  - Options: Add the full file-kind map now; or keep the current reference
    compact and enumerate the required Panta surfaces with owner links.
  - Why it matters: A table is more mechanically discoverable and auditable, but
    adds editorial weight to a short reference. A lighter entry keeps the page
    concise, but may leave readers scanning linked specs to understand all file
    kinds.
  - Evidence needed: The current style and intended depth of
    `§FS-rhei-language-reference`, whether tables are preferred in canonical
    reference pages, and whether users have trouble discovering file kinds from
    prose links alone.

- D-002: How broad the authored-surface inventory in the language reference
  should be.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 includes Panta files
    plus adjacent rhei/file kinds such as root `states.yaml`, `*.rhei.md`,
    `index.rhei.md` plus `tasks/**/*.md`, and bare rheis; codex[yolo]:openai:gpt-5.5
    focuses the recommendation on the required Panta surfaces and routes to the
    existing owner specs for the rest.
  - Options: Maintain an exhaustive or near-exhaustive plan/project file-kind
    inventory in the canonical reference; or list the Panta-specific surfaces
    needed for this decision and rely on owner specs for adjacent rhei details.
  - Why it matters: A broader inventory makes the reference a stronger user map
    for all authored files, but creates more synchronization work and may expand
    the scope beyond the Panta syntax question. A narrower inventory satisfies
    the point constraints with less churn, but may not fully solve
    cross-file-kind discoverability.
  - Evidence needed: Whether `§FS-rhei-language-reference` is intended to be a
    complete index of all authored file kinds, and whether `states.yaml` and
    bare-rhei entries belong in the same user-facing syntax surface as
    `index.panta.md`, `rheis/`, and `inbox/`.

- D-003: Which source should be cited as owning load, id, execution-root, and
  state-binding mechanics from the language reference.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 cites
    `§FS-rhei-plan-language`, `§FS-rhei-panta`, and selected architecture or
    command specs depending on the file-kind row; codex[yolo]:openai:gpt-5.5
    explicitly routes readers to `§AR-rhei-panta` for load, id, execution-root,
    and state-binding mechanics.
  - Options: Keep language-reference owner links primarily at functional-spec
    sections, with architecture links only where already necessary; or include
    `§AR-rhei-panta` as a standard destination for mechanics behind Panta
    syntax.
  - Why it matters: Functional-spec links are better for user-facing canonical
    documentation, while architecture links may be needed to fully explain ids,
    project graph loading, and state binding without duplicating implementation
    semantics.
  - Evidence needed: The repo's ownership boundary between functional specs and
    architecture specs for user documentation, and which existing spec sections
    currently declare the normative rules for load, ids, execution roots, and
    state-policy lookup.

- D-004: How concrete the ownership/update rule should be.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 proposes a specific
    obligation that adding, removing, or renaming any user-authored project or
    rhei file kind must update the file-kind map in the same change; codex[yolo]:openai:gpt-5.5
    agrees on update discipline but phrases it more generally for future Panta
    syntax changes.
  - Options: Add a precise file-kind-map synchronization rule; or add a broader
    rule that Panta authoring syntax changes update the canonical reference and
    owning spec together.
  - Why it matters: A precise rule is easier to review and validate if the table
    exists, but may be too narrow if the final reference uses prose or if syntax
    changes are not file-kind changes. A broader rule fits either rendering but
    gives reviewers less mechanical guidance.
  - Evidence needed: The final choice for D-001, and whether validation/review
    tooling can or should enforce map synchronization for file-kind changes.

## Discussion Prompt

Both proposals agree that the canonical language reference remains the single
entry point, that Panta syntax must be first-class there, and that exact grammar
should stay in owning specs. Please address only D-001 through D-004: decide
whether to add a structured file-kind map or lighter enumeration, decide how
broad the authored-surface inventory should be, identify the proper owner links
for load/id/execution/state mechanics, and choose the concrete update rule that
matches the selected structure.
