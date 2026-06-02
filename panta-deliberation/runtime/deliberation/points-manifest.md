# Discussion Points - Resolve Panta project spec conflicts

- point-inbox-hierarchy: Inbox ticket placement in the Panta hierarchy
  - Source: "The current Panta specs must not make tickets direct children of Panta if the plan-language hierarchy says level-1 children under Panta are rheis and tickets are level 2 or deeper."
  - Question: How should inbox tickets be identified, leveled, parented, and assigned state policy within the Panta hierarchy?
  - Constraints: The normative hierarchy must keep level-1 children under Panta as rheis when the plan-language hierarchy requires that shape; tickets must be level 2 or deeper; the model must define clear ids, levels, and state-policy resolution for inbox work.

- point-cross-rhei-readiness: Cross-rhei dependency readiness
  - Source: "Cross-rhei dependencies must use the same readiness rule as normal scheduling: a prior must be in a successful terminal state, meaning `final: true` and not normalized `cancelled`."
  - Question: What readiness rule should determine whether a cross-rhei dependency unblocks dependent work?
  - Constraints: Cross-rhei dependencies must follow the same rule as normal scheduling; a prerequisite must be in a terminal state with `final: true`; normalized `cancelled` prerequisites must not unblock dependent work.

- point-state-machine-resolution: Panta defaults in state-machine resolution
  - Source: "If a rhei inside a project omits `**States:**`, `index.panta.md` may supply the project default."
  - Question: What single lookup order should resolve a rhei's state machine when explicit overrides, rhei-local declarations, inherited Panta defaults, and built-in fallback may all apply?
  - Constraints: The lookup order must avoid conflicts; explicit overrides, rhei-local declarations, inherited `index.panta.md` defaults, and built-in fallback must each have a defined precedence; omitted `**States:**` on a rhei may inherit a project default.

- point-language-reference: Panta syntax in the canonical language reference
  - Source: "`index.panta.md`, Panta `rheis/` entries, and optional `inbox/` task files are user-authored language surface and should be discoverable from the canonical language reference."
  - Question: How should the canonical language reference expose and organize user-authored Panta syntax?
  - Constraints: `index.panta.md`, Panta `rheis/` entries, and optional `inbox/` task files must be documented as language surface; the documentation must be discoverable from the canonical language reference; the result must support one coherent normative model for validation, execution, ids, state-policy lookup, project defaults, and user-facing documentation.
