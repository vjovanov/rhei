# Disagreement Map - Resolve Panta defaults in state-machine resolution

## Candidate Solutions

- S-001: Single-tier precedence where the explicit invocation override is the highest-priority complete state-machine selection, followed by rhei-local `**States:**`, inherited `index.panta.md` project default, and built-in `rhei` fallback.
  - Proposed by: codex[yolo]:openai:gpt-5.5
  - Reasons: Makes operator intent highest priority for a specific validation or execution run, keeps the Panta default as a true default that fills only omitted rhei-local declarations, treats each tier as a complete non-merged selection, and lets validation and execution share one resolver with source-tier metadata.

- S-002: Two-phase resolution where the effective state-machine name is selected first from rhei-local `**States:**`, inherited `index.panta.md` default, or built-in fallback; then the file is resolved based on the source of that name, with `--state-machine <path>` acting only as a file override that must match the already-selected name.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7
  - Reasons: Separates name resolution from file location to avoid layers competing to both name and locate the machine, preserves the invariant that authored declarations select the state-machine name, allows validation and execution to share a pure resolver, and gives deterministic file roots for rhei-local, Panta-inherited, and built-in fallback cases.

## Agreements

- A-001: A rhei-local `**States:**` declaration takes precedence over an inherited `index.panta.md` project default.

- A-002: An inherited Panta default applies only when the rhei is inside a Panta project and omits its own `**States:**`.

- A-003: The built-in `rhei` state machine is the final fallback when no higher tier selects a machine.

- A-004: Resolution must be non-merged: each tier should select one complete machine/source, not combine partial choices from multiple tiers.

- A-005: Validation and execution should use the same resolver and should retain source metadata for diagnostics.

- A-006: `index.panta.md` is assumed to provide at most one project-level `**States:**` default unless the broader design introduces scoped defaults.

## Disagreements

- D-001: Whether an explicit override selects the effective state machine or only supplies the file for an already-selected name.
  - Agents: codex[yolo]:openai:gpt-5.5 vs. claude-code[yolo]:anthropic:claude-opus-4-7
  - Options: S-001 treats an explicit invocation override such as `--state-machine` as the highest-priority complete selection. S-002 treats `--state-machine <path>` as a file-level override after name resolution, requiring the file `name` to match the rhei-local, Panta-inherited, or fallback-selected name.
  - Why it matters: This determines whether operators can temporarily change the effective state machine for a run, or can only redirect to another YAML file implementing the already-declared machine name. It also changes which mistakes validation can catch when an override is present.
  - Evidence needed: The existing or intended CLI/API contract for `--state-machine`: does it mean "use this machine instead of declarations" or "load this file for the declared machine"? Also needed: current implementation behavior and any documented examples for validation/execution overrides.

- D-002: Whether explicit overrides have precedence over authored declarations.
  - Agents: codex[yolo]:openai:gpt-5.5 vs. claude-code[yolo]:anthropic:claude-opus-4-7
  - Options: S-001 orders explicit override before rhei-local `**States:**`. S-002 orders name selection from authored declarations first, then allows the explicit path only if it is name-consistent.
  - Why it matters: This affects operator control, reproducibility, and whether an invocation can mask bad or stale authored declarations.
  - Evidence needed: Product intent for explicit overrides in CI and local execution: should an override be able to force a different state policy, or should it only help locate a compatible file?

- D-003: File lookup root for a Panta-inherited `**States:**` name.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 is explicit; codex[yolo]:openai:gpt-5.5 does not specify file-root behavior.
  - Options: S-002 resolves an inherited Panta default to `<project>/states.yaml` only, and a child rhei cannot shadow it with a sibling `states.yaml` unless it redeclares `**States:**` locally. S-001 leaves open whether the inherited tier maps to a project-root file, a search path, or some other complete selection mechanism.
  - Why it matters: Validation and execution need a concrete file path rule. Allowing local shadowing would make inheritance less uniform; forbidding it makes project defaults deterministic but requires local redeclaration for local customization.
  - Evidence needed: Existing workspace layout conventions and desired Panta policy semantics: does a project default imply a project-root state-machine file, and should child rheis be able to shadow inherited defaults without local `**States:**`?

- D-004: Behavior when a declaration resolves to the literal built-in name `rhei`.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 is explicit; codex[yolo]:openai:gpt-5.5 does not specify this edge case.
  - Options: S-002 distinguishes a locally declared `**States: rhei**` from an omitted fallback: local declaration can resolve a sibling/workspace `states.yaml` named `rhei`, while omission uses the compiled built-in and ignores discovered files. S-001 only states built-in `rhei` is the final fallback and does not define whether an authored `rhei` declaration should auto-discover a file.
  - Why it matters: This affects backward compatibility for plans that declare `rhei` but provide a local `states.yaml`, and it determines whether omission and explicit declaration of the default name are observably different.
  - Evidence needed: Existing spec or tests for auto-discovery of `states.yaml` when `**States: rhei**` is declared, plus compatibility expectations for standalone and Directory Workspace plans.

- D-005: Error handling for missing or mismatched state-machine files.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 is explicit; codex[yolo]:openai:gpt-5.5 only implies diagnostics.
  - Options: S-002 says a non-`rhei` declared name with no matching file is a validation error and never falls back, and a `--state-machine` file whose `name` disagrees with the selected name is a validation error. S-001 needs equivalent error rules if tiers are complete selections, especially for override paths and inherited defaults.
  - Why it matters: The final lookup order must be usable by validation and execution; without exact failure rules, implementations may silently fall back or diverge between commands.
  - Evidence needed: Required validation behavior for missing files, mismatched YAML `name`, and whether any tier is allowed to fall through after selecting an invalid machine.

## Discussion Prompt

Address only D-001 through D-005. First decide the contract of an explicit override: complete state-machine selection or file-only redirection for a preselected name. Then define the file lookup roots and error behavior for rhei-local, Panta-inherited, explicit `rhei`, omitted fallback, and mismatched/missing files so validation and execution can share one deterministic resolver.
