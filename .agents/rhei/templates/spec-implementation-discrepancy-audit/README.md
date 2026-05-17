# Spec Implementation Discrepancy Audit Template

This template creates a directory workspace for auditing written
specifications against implementation. It partitions the audit into spec areas,
asks agents to scope each area, compare implementation behavior, elaborate
discrepancies, propose reconciliation, and then stop at a human-only decision
gate.

## Inputs

| Name | Type | Default | Description |
|---|---|---|---|
| `audit_title` | string | `Spec Implementation Discrepancy Audit` | Title for the instantiated workspace. |
| `subject` | string | `Rhei` | Product, repository, or subsystem being audited. |
| `spec_root` | string | `docs/functional-spec` | Documentation root containing the Rhei functional spec files. |
| `implementation_roots` | array<string> | `crates`, `skills`, `.agents/rhei/templates` | Root paths containing implementation, tests, templates, or skills. |
| `audit_target` | string | `codex[yolo]:openai:gpt-5.5` | Execution target for each autonomous audit state. |
| `extra_context` | string | empty | Optional context appended to the workspace overview. |

## Task Paths

| Task kind | State path | Purpose |
|---|---|---|
| Audit partition | `scope-spec` -> `compare-implementation` -> `elaborate-discrepancies` -> `propose-reconciliation` -> `human-decision` -> terminal choice | Audits one spec area and produces a human decision package. |

The terminal human choices are `update-spec`, `update-implementation`,
`update-both`, `defer-follow-up`, `no-change`, and `cancelled`.

## Flow

1. Instantiate the template inside the repository being audited.
2. Each partition starts in `scope-spec` and writes a scope inventory.
3. The comparison pass checks scoped claims against code, tests, templates,
   skills, fixtures, and command behavior.
4. The elaboration pass explains risk, impact, and evidence.
5. The proposal pass recommends a reconciliation option for every finding.
6. The human reads the proposal artifact and transitions the task to the
   chosen terminal reconciliation state.

The state machine is documented in [states.yaml](states.yaml).

## Instantiate

```bash
rhei instantiate spec-implementation-discrepancy-audit \
  --set audit_title="Spec Implementation Discrepancy Audit" \
  --set subject="Rhei" \
  --set audit_target="codex[yolo]:openai:gpt-5.5" \
  --output docs/plans/spec-implementation-discrepancy-audit
```

For non-scalar inputs, use a values file:

```bash
rhei instantiate spec-implementation-discrepancy-audit \
  --values audit-values.yaml \
  --output docs/plans/spec-implementation-discrepancy-audit
```

See the pre-rendered example at
`examples/spec-implementation-discrepancy-audit-example/`.
