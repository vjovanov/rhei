# analyze-and-dispatch — example

A pre-rendered instantiation of the
[`analyze-and-dispatch`](../../.agents/rhei/templates/analyze-and-dispatch/)
template. This is the canonical reference for **dynamic (agent-driven) task
creation**: a single coordinator task analyzes a subject and writes the
follow-up tasks into the workspace at run time.

As shipped the workspace has only the coordinator (`analyze`) task — the
dispatched tasks do not exist until the coordinator actually runs and writes
them, so a fresh `--dry-run` shows just the one ready task.

## Inputs used

Checked in at `instantiation-values.yaml`. The coordinator is pointed at this
repo's spec directory and asked to dispatch one task per spec missing an
example.

## Validate

```bash
rhei validate examples/analyze-and-dispatch-example
rhei run examples/analyze-and-dispatch-example --dry-run   # only `analyze` is ready
```

## Seeing the dispatched tasks (simulated pass)

The dispatched tasks are written by the coordinator agent at run time. To see
the shape they take without running an agent, simulate one pass in a throwaway
copy — mark the coordinator complete and drop in the files it would write:

```bash
tmp="$(mktemp -d)/ws"; cp -r examples/analyze-and-dispatch-example "$tmp"
sed -i 's/\*\*State:\*\* analyze/**State:** completed/' "$tmp/tasks/01-analyze.md"
cat > "$tmp/tasks/02-alpha.md" <<'EOF'
### Task alpha: Add an example for one spec
**State:** address
**Prior:** Task analyze

Add a runnable example for the spec.
EOF
cat > "$tmp/tasks/03-report.md" <<'EOF'
### Task report: Summarize dispatched work
**State:** report
**Prior:** Task alpha

Summarize the added examples.
EOF
rhei validate "$tmp"
rhei run "$tmp" --parallel 4 --dry-run   # alpha ready; report waits on alpha
rm -rf "$(dirname "$tmp")"
```

`address` is a `concurrent: true` state, so with several dispatched tasks
`--parallel N` works them simultaneously while the `report` task waits for all.

## Regenerate

```bash
rm -rf examples/analyze-and-dispatch-example
rhei instantiate .agents/rhei/templates/analyze-and-dispatch \
  --values .agents/rhei/templates/analyze-and-dispatch/.example-values.yaml \
  --output examples/analyze-and-dispatch-example
```

After regenerating, restore this README and `instantiation-values.yaml` from the
checked-in copy if you want to keep the example metadata alongside the rendered
workspace.
