# Proposal - claude-code[yolo]:anthropic:claude-opus-4-7

- Recommendation: Restructure `§FS-rhei-language-reference` so the "Plan and
  project markdown" surface is presented as a small **file-kind map** organized
  by container scope (Project → Rhei → Workspace tasks → Inbox). Each row names
  one user-authored file or directory, gives a one-line role description, and
  cites the single owning spec section where its normative rules live. Grammar
  stays in `§FS-rhei-plan-language`; project semantics stay in `§FS-rhei-panta`;
  the language reference remains a discoverability page that links to them.

  Concretely, replace the current single "Plan and project markdown" bullet
  with a structured map of the form:

  | Layer            | Authored surface                  | Role                           | Owner |
  |------------------|-----------------------------------|--------------------------------|-------|
  | Project (Panta)  | `index.panta.md`                  | project manifest               | §FS-rhei-panta.1, §FS-rhei-plan-language.1.5 |
  | Project (Panta)  | `rheis/` directory                | container for the project's rheis | §FS-rhei-plan-language.1.5 |
  | Project (Panta)  | `inbox/` directory (optional)     | loose tickets, loaded as synthetic `inbox` rhei | §FS-rhei-plan-language.1.5, §FS-rhei-panta.2 |
  | Project (Panta)  | `states.yaml` at project root     | project-default state machine  | §FS-rhei-panta.6, §FS-rhei-plan-language.1.3 |
  | Rhei (single)    | `*.rhei.md`                       | one rhei in one file           | §FS-rhei-plan-language.1.1 |
  | Rhei (workspace) | `index.rhei.md` + `tasks/**/*.md` | one rhei across many files     | §FS-rhei-plan-language.1.2 |
  | Bare rhei        | a `.rhei.md` outside a project    | implicit Panta of one          | §AR-rhei-panta |

  Update the **Reading Path** so project-layer files are introduced before
  rhei-layer files (container-to-contained order): start at §FS-rhei-authoring,
  then §FS-rhei-panta for project layout, then §FS-rhei-plan-language sections
  1.5 (Panta Project) → 1.1/1.2 (rhei formats) → 1.3 (state-machine
  resolution) → 2/3 (grammar and semantic constraints).

  Extend **Ownership Rules** (§FS-rhei-language-reference.3) with one new
  obligation: any change that adds, removes, or renames a user-authored
  *file kind or directory* at the project or rhei level must update the
  file-kind map in §FS-rhei-language-reference in the same change as the
  owning spec edit. The map is the language reference's index of file kinds;
  if it goes stale, discoverability is broken even if the grammar is correct.

- Reasons:
  - **Direct answer to the question.** The point asks "how should the
    canonical language reference expose and organize user-authored Panta
    syntax?" A file-kind map exposes each Panta file by name in the entry-
    point spec, with one click to the owning normative rule. The current
    text mentions the files in a prose bullet, which forces a reader to
    scan §FS-rhei-plan-language to discover what files exist.
  - **Discoverability without duplication.** The map names files and points
    at the single normative source for each. No grammar is copied into the
    language reference, so the spec cannot drift between two homes.
  - **Layered organization matches the load model.** Project → rhei →
    workspace task → inbox mirrors the Panta → rhei → ticket hierarchy the
    rest of the spec already uses (§FS-rhei-panta.1). A reader who learns
    "files are organized by container scope" can predict where to look for
    new constructs as the language grows.
  - **Supports the coherent normative model.** Every constraint in the
    point statement (validation, execution, ids, state-policy lookup,
    project defaults, user-facing behavior) is already specified somewhere;
    the map's job is to make those locations easy to find:
    - validation → §FS-rhei-validate (linked from the reference's command
      section, which already exists).
    - execution → §FS-rhei-run / §FS-rhei-panta.6.2.
    - ids → §FS-rhei-panta.5 / §AR-rhei-panta.
    - state-policy lookup → §FS-rhei-plan-language.1.3 (the row for
      `states.yaml` in the map points there).
    - project defaults → §FS-rhei-panta.6 / §FS-rhei-plan-language.1.3.
    - user-facing behavior → §FS-rhei-authoring + §FS-rhei-panta.4.
  - **Cheap to apply.** The current §FS-rhei-language-reference is 66 lines.
    Inserting one table and one extended ownership clause is a contained edit
    that does not perturb the larger grammar or panta specs.
  - **Ownership rule makes future drift a `rhei validate`-able expectation.**
    "Update the map in the same change" is concrete review guidance; reviewers
    can mechanically check it.

- Tradeoffs:
  - **File names appear in two places** (the map and §FS-rhei-plan-language.1.5).
    Mitigation: the map is one-liners-plus-citations, never normative grammar;
    the row for `inbox/` says *"loose tickets, loaded as synthetic `inbox`
    rhei"* and cites §FS-rhei-plan-language.1.5, but the loading rule itself
    is only written once.
  - **One extra editorial step per language change.** Adding a new file kind
    now requires editing both the owner spec and the map. Acceptable cost for
    the discoverability gain; this is exactly what the extended ownership rule
    is for.
  - **Tables are markdown-heavy.** Some reviewers prefer prose-bullet style
    throughout. Mitigation: the table can be rendered as a bulleted list with
    the same fields if the project's spec style guide rejects tables; the
    structure (layer / surface / role / owner) is what matters, not the
    rendering.
  - **The map will need a tiny update if point-inbox-hierarchy chooses
    "drop inbox in v1"** — the inbox row would be removed. Trivial.
  - **Does not unify the panta and plan-language specs.** This proposal keeps
    them as two distinct normative documents and only improves the map. A
    more ambitious refactor (fold §FS-rhei-panta sections 1, 5, 6 into
    §FS-rhei-plan-language) would yield a single grammar+semantics document
    but is out of scope for the point at hand and risks merging two readable
    specs into one over-large one.

- Assumptions:
  - The current three-spec layering is correct and should be preserved:
    `§FS-rhei-language-reference` is the entry-point/map page,
    `§FS-rhei-plan-language` owns grammar and load semantics,
    `§FS-rhei-panta` owns project-root semantics and command scope.
    Refactoring that layering is a larger architecture call not driven by
    this point.
  - The synthetic `inbox` rhei model proposed in
    [[point-inbox-hierarchy]] is the load semantics. The language reference
    only has to *name* `inbox/` as an authored surface and point at the spec
    section that defines the synthesis rule — it does not encode the synthesis
    itself.
  - The state-machine resolution order (per [[point-state-machine-resolution]])
    is documented in §FS-rhei-plan-language.1.3 and §FS-rhei-panta.6, and the
    `states.yaml` row in the map cites those locations without redefining the
    precedence.
  - Cross-rhei readiness ([[point-cross-rhei-readiness]]) lives in
    §FS-rhei-panta.6.1; the language reference does not need a row for it
    because readiness is a runtime rule, not an authored file kind.
  - Tables are an acceptable markdown construct in this repo's specs (the
    plan-language spec already uses a table for "Single-File Plan" components).

- Rejection criteria:
  - **Reject if the team prefers prose over structured maps in the language
    reference.** If §FS-rhei-language-reference must remain a short narrative
    page with file kinds only mentioned in prose, drop the table and keep only
    the extended ownership rule.
  - **Reject if Panta files should not be promoted to first-class entries
    in the language reference.** If the consensus is "the language reference
    is rhei-level only and project-level syntax is one section among many
    inside §FS-rhei-plan-language", then this proposal is moot — the fix
    instead is to add a `§FS-rhei-language-reference.1` sub-bullet that links
    to §FS-rhei-plan-language.1.5 and stop there.
  - **Reject if a separate `§FS-rhei-panta-reference` spec is introduced**
    as a sibling to §FS-rhei-plan-language. In that world the file map for
    project-level files belongs in the new spec, and §FS-rhei-language-
    reference only links to the two reference specs (rhei and panta).
  - **Reject if `inbox/` is cut from v1.** The inbox row of the map is then
    removed; the rest of the proposal still applies.
  - **Reject if file-kind discoverability is judged adequately served by
    §FS-rhei-plan-language.1.5 alone.** That section already lists the three
    Panta-level surfaces; if reviewers think one extra link from
    §FS-rhei-language-reference to §FS-rhei-plan-language.1.5 closes the
    discoverability gap, the full map is overkill.
