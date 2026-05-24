# Multi-Model Analysis Template

This template instantiates a single-task Rhei workspace that:

1. creates one task with a single `analyze` state that fans out across three
   bundled targets by default
2. stores one note per target under `runtime/analyses/`
3. advances the same task to a `summarize` state after the fanout completes
4. writes the final document to `runtime/final-analysis.md` by default

The instantiated workspace also ships a default `.rhei/settings.json` with
`defaults.agent_timeout` so `rhei run` can orchestrate the analysis agents
without extra manual setup.

Instantiate it from the repository root:

```bash
cargo run -p rhei-cli -- instantiate multi-model-analysis \
  --set plan_title="Multi-Model Analysis" \
  --set task_title="Analyze the target problem" \
  --set-file task_description=./brief.md \
  --output ./multi-model-analysis-demo
```

To override the fanout target set, use a values file so the structured
`agents` input stays readable:

```bash
cargo run -p rhei-cli -- instantiate multi-model-analysis \
  --values ./multi-targets.yaml \
  --set plan_title="Multi-Model Analysis" \
  --set task_title="Analyze the target problem" \
  --set-file task_description=./brief.md \
  --output ./multi-model-analysis-demo
```

Example `multi-targets.yaml`:

```yaml
agents:
  - id: claude
    label: Claude analysis note
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: gemini
    label: Gemini flash analysis note
    selector: gemini[yolo]:google:gemini-3.1-flash
  - id: codex
    label: Codex analysis note
    selector: codex[yolo]:openai:gpt-5-codex

summary_agent: claude-code[yolo]:anthropic:claude-opus-4-7
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
Specification](../../../../docs/functional-spec/rhei-agents.spec.md) for the
full schema and mode-resolution order.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `plan_title` | string | *(required)* | Title of the instantiated plan (index heading). |
| `task_title` | string | *(required)* | Title of the shared analysis task. |
| `task_description` | string | *(required)* | Instructions given to every analysis pass. Use `--set-file` for long briefs. |
| `analysis_output_dir` | string | `runtime/analyses` | Where per-target notes are written. |
| `agents` | object[] | 3 targets (claude/gemini/codex) | The fan-out set. Each entry is `{ id, label, selector }`; one analysis pass runs per entry. `id` is slugified into artifact paths. |
| `summary_agent` | string | `claude-code[yolo]:…opus-4-7` | Agent that writes the final synthesis. |
| `final_document_path` | string | `runtime/final-analysis.md` | Where the synthesis lands. |

Per-task path: a single `analysis` task runs `analyze` (fan-out across
`agents`, one pass per target) → `summarize` (one synthesis pass). Run it with
`rhei run --parallel <n>` to execute the per-target passes concurrently.

## Note

This template mirrors the `target` / `all_targets` spec shape: a single task,
one fan-out analysis state, and one synthesis state. The manifest uses
MiniJinja-compatible structured inputs, so `agents` is a real array of objects
rather than a bundle of hardcoded scalar parameters.

It uses the directory-workspace layout rather than a single-file plan so
`rhei run --parallel <n>` can execute the fanout without being forced back to
sequential mode.

## Example

A pre-rendered example lives at
[`examples/multi-model-analysis-example/`](../../../../examples/multi-model-analysis-example/)
and passes `rhei validate` as shipped.
