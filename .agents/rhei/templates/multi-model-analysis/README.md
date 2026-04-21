# Multi-Model Analysis Template

This template instantiates a single-task Rhei plan that:

1. creates one analysis task per bundled model pass for `claude`, `gemini`,
   and `codex`
2. stores one note per model under `runtime/analyses/`
3. creates a final `claude` synthesis task that depends on all analysis notes
4. writes the final document to `runtime/final-analysis.md` by default

Instantiate it from the repository root:

```bash
cargo run -p rhei-cli -- instantiate multi-model-analysis \
  --set plan_title="Multi-Model Analysis" \
  --set task_title="Analyze the target problem" \
  --set-file task_description=./brief.md \
  --output ./multi-model-analysis-demo
```

To override the Gemini model used for the Gemini analysis pass:

```bash
cargo run -p rhei-cli -- instantiate multi-model-analysis \
  --set plan_title="Multi-Model Analysis" \
  --set task_title="Analyze the target problem" \
  --set-file task_description=./brief.md \
  --set gemini_model="gemini-3.1-pro-preview" \
  --output ./multi-model-analysis-demo
```

## Customizing the Gemini CLI

Each state references a coding-agent id (`claude-code`, `gemini`, `codex`) that
Rhei resolves against the merged `agents` registry. The built-in profiles are
usually enough, but if your local Gemini CLI uses a different binary name or
different prompt/model flags, override the `gemini` entry in
`<plan>/.rhei/settings.json`:

```json
{
  "agents": {
    "gemini": {
      "command": ["your-gemini-cli"],
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "stdin_prompt": false,
      "modes": {
        "yolo": ["--approval-mode", "auto_edit"]
      }
    }
  }
}
```

A user-written entry replaces the built-in `gemini` profile wholesale — keep
the fields you need. See [Agents
Specification](../../../../docs/specs/rhei-agents.spec.md) for the full
schema and mode-resolution order.
