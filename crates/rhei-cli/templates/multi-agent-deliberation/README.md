# Multi-Agent Deliberation Template

This template turns a discussion, transcript, issue, or decision brief into a
structured multi-agent deliberation. One splitter agent breaks the input into
separate points. Configured target agents independently propose a solution for
each point, an aggregator tags agreements and disagreements, the targets discuss
those disagreements, and an aggregator chooses a point decision. A final
aggregator then proposes the overall solution, and a presentation agent writes
the simplest human-facing summary with the reasons for the choice.

The custom workflow is defined in [`states.yaml`](states.yaml), including the
state-machine diagram.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `plan_title` | string | `Multi-Agent Deliberation` | Title of the instantiated workspace. |
| `discussion_title` | string | required | Short title for the discussion being resolved. |
| `discussion` | string | required | Full discussion/transcript/brief. Use `--set-file` for long input. |
| `splitter_agent` | string | `codex[yolo]:openai:gpt-5.5` | Agent that turns the input into point tasks. |
| `target_agents` | object[] | claude/codex/gemini | Agents that propose and discuss solutions for every point. Each entry is `{ id, label, selector }`; the array must not be empty, and both `id` and `selector` values must be unique. |
| `point_aggregator_agent` | string | `codex[yolo]:openai:gpt-5.5` | Agent that tags disagreements and resolves each point. |
| `final_aggregator_agent` | string | `codex[yolo]:openai:gpt-5.5` | Agent that synthesizes the final cross-point solution. |
| `presentation_agent` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Agent that writes the simplest human-facing summary. |
| `max_points` | positive number | `8` | Soft cap for how many point tasks the splitter should create. |
| `output_dir` | string | `runtime/deliberation` | Workspace-relative artifact directory. |

## Task Paths

| Task kind | State path |
|---|---|
| Splitter task | `split-points -> completed` |
| Generated point task | `propose-solutions -> aggregate-disagreements -> discuss-point -> resolve-point -> completed` |
| Final aggregation task | `final-solution -> present-to-human -> human-review -> completed` |

## Flow

1. `Task split` reads the discussion, writes a point manifest, creates one
   `point-<slug>` task per separate point, and creates `Task final-solution`
   with priors on every point task.
2. Each point task fans out through `propose-solutions`, running every configured
   `target_agents` entry and writing one proposal per target.
3. `aggregate-disagreements` reads the proposals, groups candidate solutions,
   names agreements, tags disagreements, and writes a discussion prompt.
4. `discuss-point` fans out to the same target agents so they respond to the
   disagreement map rather than restating their first proposal.
5. `resolve-point` chooses the best solution for that point and records why.
6. `final-solution` reads all point decisions and writes the overall
   recommendation.
7. `present-to-human` writes `runtime/deliberation/human-summary.md`, then the
   final task enters the `human-review` gate. A human accepts by transitioning to
   `completed` or abandons by transitioning to `cancelled`.

## Instantiate

```bash
cargo run -p rhei-cli -- instantiate multi-agent-deliberation \
  --set discussion_title="Resolve API design discussion" \
  --set-file discussion=./discussion.txt \
  --output ./multi-agent-deliberation-demo
```

For custom target agents, use a values file:

```yaml
target_agents:
  - id: claude
    label: Claude proposal
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: codex
    label: Codex proposal
    selector: codex[yolo]:openai:gpt-5.5
  - id: gemini
    label: Gemini proposal
    selector: gemini[yolo]:google:gemini-3.1-pro-preview
```

Then instantiate with:

```bash
cargo run -p rhei-cli -- instantiate multi-agent-deliberation \
  --values ./deliberation-values.yaml \
  --set-file discussion=./discussion.txt \
  --output ./multi-agent-deliberation-demo
```

## Example

A pre-rendered example lives at
[`examples/multi-agent-deliberation-example/`](../../../../examples/multi-agent-deliberation-example/)
and passes `rhei validate` as shipped.
