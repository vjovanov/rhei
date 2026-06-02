# Proposal - codex[yolo]:openai:gpt-5.5

- Recommendation: Resolve a rhei's state machine with this precedence:
  explicit invocation override, then rhei-local `**States:**`, then inherited
  `index.panta.md` project default, then the built-in `rhei` fallback. Treat
  each tier as a complete selection, not a merge. Validation and execution
  should call the same resolver and record the selected source tier.
- Reasons: This makes "closest authored declaration wins" the normal rule while
  still letting operators force a machine for a particular validation or
  execution run. A Panta default then behaves exactly like a default: it fills
  only missing rhei-local declarations. The built-in machine remains the final
  compatibility path for standalone plans and Panta projects that intentionally
  omit project policy.
- Tradeoffs: A project default cannot force uniform state policy when a child
  rhei declares its own `**States:**`; that must be handled by a separate
  validation policy if needed. Explicit overrides can hide declaration mistakes
  during an ad hoc run, so diagnostics should include "resolved from override"
  and normal validation without the override should still check authored
  declarations. The resolver must carry source metadata, which is slightly more
  implementation work than returning only a state-machine name.
- Assumptions: An explicit override means a CLI/API override supplied for the
  current operation, such as a `--state-machine` argument. A rhei is "inside a
  Panta project" only when it is discovered through that project's manifest or
  directory model. `index.panta.md` has at most one project-level `**States:**`
  declaration; if it has none, the inherited-default tier is skipped.
- Rejection criteria: Do not use this proposal if project authors need
  `index.panta.md` to forcibly override child rhei declarations by default, or
  if execution and validation intentionally need different state-machine
  precedence. Also reject it if explicit overrides are meant only to locate the
  YAML file for an already-selected `**States:**` name rather than to select the
  effective state machine itself.
