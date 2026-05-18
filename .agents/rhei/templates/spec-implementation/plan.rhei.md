# Rhei: {{spec_title}} — {{spec_path}}
**States:** spec-implementation

## What this workspace does

Implement the specification at `{{spec_path}}` end-to-end and walk it through:

1. **Implementation** by `{{implementation_target}}`.
2. **Completeness audit** — each reviewer independently checks whether the
   implementation covers every normative claim in the spec; `{{smart_target}}`
   merges the per-reviewer findings into one gap list; `{{implementation_target}}`
   closes the gaps.
3. **Quality review/fix loop** — `{{review_passes}}` cycles. Each cycle:
   - every reviewer reviews the implementation in parallel,
   - `{{smart_target}}` writes a fix plan,
   - `{{smart_target}}` applies the accepted fixes.
4. **End-to-end coverage loop** — `{{e2e_passes}}` cycles:
   - `{{e2e_writer}}` writes / extends e2e tests against the mock agent
     (`{{mock_agent}}`),
   - `{{e2e_verifier}}` re-runs the standard suite, audits the new tests,
     enforces the mock-agent policy, and lists remaining gaps for the next
     write pass.

## Configuration

| Role | Target |
|---|---|
| Implementer | `{{implementation_target}}` |
| Reviewers (fan-out) | {% for t in review_targets %}`{{ t }}`{% if not loop.last %}, {% endif %}{% endfor %} |
| Smart target (aggregate, fix) | `{{smart_target}}` |
| E2E writer | `{{e2e_writer}}` |
| E2E verifier | `{{e2e_verifier}}` |

Quality loop cycles: **{{review_passes}}** &nbsp;·&nbsp; E2E loop cycles: **{{e2e_passes}}**
{%- if focus_areas %}

Quality reviewers must address these focus areas:

{%- for f in focus_areas %}
- `{{ f }}`
{%- endfor %}
{%- endif %}
{%- if e2e_test_root %}

End-to-end tests live under `{{e2e_test_root}}`.
{%- endif %}

## E2E test policy

Every newly-added e2e test MUST target the mock agent (`{{mock_agent}}`),
which returns canned outputs the test controls. The standard suite stays
fast, deterministic, and offline.

Tests that exercise real agent operations are reserved for a small
release-only subset, marked with `{{release_only_marker}}` so CI can exclude
them from the default test command and include them only in release builds.
{%- if release_only_test_root %} Those tests live under
`{{release_only_test_root}}`.{% endif %} The rule of thumb is one
happy-path test per distinct real-agent integration — the verifier flags
growth beyond that.

## Where work happens

This workspace is a **scratchpad**. Every state resolves the repository root
with `git rev-parse --show-toplevel` and applies code edits in the repository
checkout. Runtime artifacts (`runtime/...`) stay under this workspace.

## Tasks

### Task spec-implementation: Implement {{spec_path}}
**State:** implement

Implement the spec at `{{spec_path}}` end-to-end and walk it through the
completeness pass, the quality review/fix loop, and the e2e coverage loop
defined in `states.yaml`.
