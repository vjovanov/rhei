# spec-review — example

A pre-rendered instantiation of the
[`spec-review`](../../.agents/rhei/templates/spec-review/) template, used as a
smoke test that the template produces a valid workspace. This is the canonical
**counted-loop** reference: one task that runs `review → fix` twice and then
completes.

## Inputs used

```yaml
spec: specs/template-review-fixture.spec.md
criteria: thread safety, backward compatibility
```

The same values are checked in at `instantiation-values.yaml`. The fixture spec
is bundled under `specs/`, so the example remains portable across checkout
locations and temporary e2e copies.

<!-- Fixture grounding for repository validation: §FS-template-review-fixture -->

## Validate

```bash
rhei validate examples/spec-review-example
rhei run examples/spec-review-example --dry-run
```

## Regenerate

```bash
rm -rf examples/spec-review-example
rhei instantiate .agents/rhei/templates/spec-review \
  --values .agents/rhei/templates/spec-review/.example-values.yaml \
  --output examples/spec-review-example
```

After regenerating, restore this README and `instantiation-values.yaml` from the
checked-in copy if you want to keep the example metadata alongside the rendered
workspace.
