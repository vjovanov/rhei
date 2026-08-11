# spec-review — example

A pre-rendered instantiation of the
[`spec-review`](../../crates/rhei-cli/templates/spec-review/) template, used as a
smoke test that the template produces a valid workspace. This is the canonical
**counted-loop** reference: one task that runs `review → fix` twice and then
completes.

## Inputs used

```yaml
spec: specs/template-review-fixture.spec.md
criteria: thread safety, backward compatibility
```

The same values are checked in at `instantiation-values.yaml`. The fixture spec
is an example-owned file bundled under `specs/`, so the example remains portable
across checkout locations and temporary e2e copies. Instantiating the template
does not produce it — a real instantiation reviews the spec the `spec` input
names, and ships no demo data.

<!-- Fixture grounding for repository validation: §FS-template-review-fixture -->

## Validate

```bash
rhei validate examples/spec-review-example
rhei run examples/spec-review-example --dry-run
```

## Regenerate

```bash
rm -rf examples/spec-review-example
rhei instantiate crates/rhei-cli/templates/spec-review \
  --values crates/rhei-cli/templates/spec-review/.example-values.yaml \
  --output examples/spec-review-example
```

After regenerating, restore this README, `instantiation-values.yaml`, and
`specs/template-review-fixture.spec.md` from the checked-in copies — all three
are example-owned files that instantiation does not produce.
