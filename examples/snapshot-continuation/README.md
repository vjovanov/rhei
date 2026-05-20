# Snapshot Continuation Example

This example shows a same-agent implementation/review flow with session
snapshots enabled.

The `implement` state writes a named `implementation` snapshot, and the
`review` state requires that snapshot as inherited context. The same snapshot
can also be opened by an operator with `rhei snapshot continue` for analysis
without changing task state or advancing the `current` pointer.

The bundled `scripts/fake-analysis-agent.sh` implements the minimum native
session contract needed to run the example locally. Replace that command and
flags with a real agent profile when adapting the workflow to production.

## Commands

Validate the example:

```bash
cargo xtask examples validate snapshot-continuation
```

Run the workflow:

```bash
cargo run -p rhei-cli -- \
  --state-machine examples/snapshot-continuation/states.yaml \
  run examples/snapshot-continuation --no-tui
```

List all orchestrator and operator generations:

```bash
cargo run -p rhei-cli -- \
  --state-machine examples/snapshot-continuation/states.yaml \
  snapshot list \
  --plan examples/snapshot-continuation \
  --produced-by all
```

Continue interactively from the implementation snapshot:

```bash
cargo run -p rhei-cli -- \
  --state-machine examples/snapshot-continuation/states.yaml \
  snapshot continue \
  1:implementation:implement@1:analysis-agent-acme-model-a \
  --plan examples/snapshot-continuation
```

Inspect the captured operator generation:

```bash
cargo run -p rhei-cli -- \
  --state-machine examples/snapshot-continuation/states.yaml \
  snapshot show \
  1:implementation:implement@1:analysis-agent-acme-model-a/g2 \
  --plan examples/snapshot-continuation
```

Run review from a specific snapshot generation:

```bash
cargo run -p rhei-cli -- \
  --state-machine examples/snapshot-continuation/states.yaml \
  run examples/snapshot-continuation \
  --no-tui \
  --from-snapshot 1:implementation:implement@1:analysis-agent-acme-model-a/g1
```
