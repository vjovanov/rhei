# Living Review Loop Example

This workspace demonstrates a living Rhei that uses `all_models` to run a
multi-model review pass before the orchestrator consolidates findings.

The flow is:

1. `review-seed` starts in the `reviewing` state, which declares
   `all_models: [claude, codex]`.
2. The runtime calls `workflow.sh write-review` twice — once per model —
   with `RHEI_MODEL` set to `claude` and `codex` in turn.
3. Each model writes its findings to `runtime/findings/<model>-findings.md`.
4. Once all model callbacks complete, `review-seed` advances to `consolidating`.
5. The coordinator runs on `codex`, merges the per-model files into a single
   `runtime/findings/review-findings.md` and appends one verification task per
   consolidated review point.
6. Each verification task runs on `codex` and writes a reproduction note under
   `runtime/verifications/`.
7. Only findings marked relevant cause the orchestrator to append new fix task
   files.
8. The fix tasks then run and write completion artifacts under `runtime/fixes/`.

By default the example stays deterministic and writes canned findings so the
checked-in tests do not depend on local model credentials. Set
`RHEI_LIVING_REVIEW_MODE=live` to make `workflow.sh` dispatch to the local
`claude` and `codex` CLIs instead.

Validate the checked-in workspace from the repository root:

```bash
cargo run -p rhei-cli -- --state-machine examples/living-review-loop/team-states.yaml validate examples/living-review-loop
```

Run a disposable copy so the orchestrator can append task files without
modifying the tracked example:

```bash
tmp_dir="$(mktemp -d)"
cp -R examples/living-review-loop "$tmp_dir/living-review-loop"
cargo run -p rhei-cli -- --state-machine "$tmp_dir/living-review-loop/team-states.yaml" run "$tmp_dir/living-review-loop"
```

`rhei reset` resets task states and removes `runtime/` (including results),
but it does not delete task files that were appended dynamically during the
run. For a clean rerun, delete the disposable copy and copy the example again.
