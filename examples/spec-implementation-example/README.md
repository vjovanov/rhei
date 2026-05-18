# spec-implementation - example

A pre-rendered instantiation of the
[`spec-implementation`](../../.agents/rhei/templates/spec-implementation/)
template used as a smoke test that the template produces a valid workspace.

## Inputs used

```yaml
spec_path: docs/functional-spec/rhei-list.spec.md
spec_title: rhei-list Spec Implementation Example
implementation_target: claude-code[yolo]:anthropic:claude-opus-4-7
review_targets:
  - claude-code[yolo]:anthropic:claude-opus-4-7
  - codex[xhigh]:openai:gpt-5.5
smart_target: codex[xhigh]:openai:gpt-5.5
review_passes: 2
focus_areas:
  - performance
  - error handling
  - concurrency
e2e_writer: claude-code[yolo]:anthropic:claude-opus-4-7
e2e_verifier: codex[xhigh]:openai:gpt-5.5
e2e_passes: 2
e2e_test_root: e2e
mock_agent: mock
release_only_marker: release-only
release_only_test_root: e2e/release-only
```

The same values are checked in at `instantiation-values.yaml`.

## Validate

```bash
rhei validate examples/spec-implementation-example/plan.rhei.md
rhei run examples/spec-implementation-example/plan.rhei.md --dry-run
```

## Regenerate

```bash
rm -rf examples/spec-implementation-example
rhei instantiate .agents/rhei/templates/spec-implementation \
  --values examples/spec-implementation-example/instantiation-values.yaml \
  --output examples/spec-implementation-example
```

After regenerating, restore this README and the checked-in
`instantiation-values.yaml` if the generator overwrote them.
