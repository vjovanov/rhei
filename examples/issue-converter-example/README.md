# issue-converter-example

A committed instantiation of the [`issue-converter`](../../.agents/rhei/templates/issue-converter)
template. It converts a bounded GitHub issue queue into executable Rhei task
chains (spec-inspection → implementation → verification → PR), one per matching
non-duplicate issue, each in its own git worktree.

The values used to render this example are in
[`instantiation-values.yaml`](instantiation-values.yaml): repository
`octocat/hello-world`, GitHub Project `octocat/1`, converting up to 5 `Todo`
issues per batch with a 10m sleep between batches.

## Run

```sh
rhei run issues-octocat --parallel 2
```

`--parallel 2` (or higher) is required: the converter parks one worker in
`wait-for-next-batch` between batches while generated issue chains advance in
their own worktrees.

## Regenerate

This example is drift-gated against the template — regenerate it after any
template change and commit the result:

```sh
rhei instantiate .agents/rhei/templates/issue-converter \
  --values .agents/rhei/templates/issue-converter/.example-values.yaml \
  --output examples/issue-converter-example
cp .agents/rhei/templates/issue-converter/.example-values.yaml \
  examples/issue-converter-example/instantiation-values.yaml
```

(The example owns this `README.md` and `instantiation-values.yaml`; every other
file is rendered template output.)
