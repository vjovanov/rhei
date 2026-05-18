# product-management - example

A pre-rendered instantiation of the
[`product-management`](../../.agents/rhei/templates/product-management/)
template used as a smoke test that the template produces a valid workspace.

This example uses the default two-pass loop with two PM targets: Claude Code
and Codex. The `pm_targets` input is still configurable; this example simply
keeps the shipped target set small.

## Inputs used

```yaml
plan_title: Rhei Product Management Run
product_name: Rhei
product_brief: |
  Improve the Rhei authoring and execution experience for teams that use
  agent-driven plans. Focus on predictable execution, useful monitoring, and
  templates that are easy to instantiate repeatedly.
implementation_scope: "docs/functional-spec, .agents/rhei/templates, examples"
pm_targets:
  - id: claude
    label: Claude PM entries
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: codex
    label: Codex PM entries
    selector: codex[xhigh]:openai:gpt-5.5
smart_target: codex[xhigh]:openai:gpt-5.5
implementation_target: codex[medium]:openai:gpt-5.4-mini
loop_passes: 2
focus_areas:
  - template usability
  - monitoring clarity
  - predictable execution
validation_criteria:
  - user value is explicit
  - evidence or assumption is stated
  - scope fits one implementation pass
  - conflicts with existing specs or roadmap are identified
max_entries_per_pass: 2
```

The same values are checked in at `instantiation-values.yaml`.

## Validate

```bash
rhei validate examples/product-management-example
rhei run examples/product-management-example --dry-run
```

## Regenerate

```bash
rm -rf examples/product-management-example
rhei instantiate .agents/rhei/templates/product-management \
  --values examples/product-management-example/instantiation-values.yaml \
  --output examples/product-management-example
```

After regenerating, restore this README and the checked-in
`instantiation-values.yaml` if the generator overwrote them.
