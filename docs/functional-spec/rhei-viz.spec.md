# FS-rhei-viz: `rhei viz`

Render an interactive, browser-based visualization of a rhei plan (or a workspace of plans). The view shows the three Rhei levels — plan, task, subtask — and their states, laid out so a human can see the whole plan's shape at a glance.

For the plan grammar see the [Plan Language Specification](rhei-plan-language.spec.md). For the state machine see the [States Specification](rhei-states.spec.md). For the TUI that runs alongside `rhei run`, see [Run TUI](rhei-run-tui.spec.md) — `rhei viz` is a distinct command aimed at static plan inspection, not live parallel execution.

## Goals

1. **Make plan shape legible.** One page shows every task, every subtask, and each element's current state — without the user scrolling through markdown.
2. **Separate state vocabularies by level.** Plan, task, and subtask states often draw from different sets (the default machine uses different states at task vs subtask granularity). The visualization keeps them visually distinct so a `review` at level 1 is not confused with a `needs-review` at level 2.
3. **Be self-contained.** The rendered artifact is a single HTML file. No network access, no CDN dependencies, no build step at view time.
4. **Be live when requested.** Under `--serve`, the view updates when the underlying `.rhei.md` file changes on disk, so a plan author can iterate with the view open in a second window.

## Non-Goals

- **Not a replacement for `rhei run`'s TUI.** `rhei viz` does not follow live execution; it snapshots the plan as it is on disk. For live parallel-run visualization, use `rhei run --tui` (see [Run TUI](rhei-run-tui.spec.md)).
- **Not a plan editor.** Clicks navigate and filter; they do not mutate the plan. Editing remains the responsibility of `rhei` CLI commands or a text editor.
- **No remote deployment.** The optional server binds to loopback only.

## Usage

```
rhei viz [PATH]                    # default: current workspace or ./*.rhei.md
rhei viz plan.rhei.md              # single plan
rhei viz ./docs/plans              # directory — auto-discovers *.rhei.md
rhei viz --output viz.html         # write the HTML and do not open a browser
rhei viz --serve                   # start the live-reload server and open browser
rhei viz --no-open                 # emit HTML, skip the browser launch
```

Default behavior when `PATH` is omitted: resolve the workspace via the same rules as `rhei run`. If the workspace contains one or more `.rhei.md` files, all are loaded and exposed via a plan selector in the UI.

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `PATH` | (workspace) | A single `.rhei.md` file, a directory of plans, or omitted to use the current workspace. |
| `--output <PATH>` | — | Write the rendered HTML to `<PATH>` and exit. Implies `--no-open`. |
| `--serve` | `false` | Start a local HTTP server on `127.0.0.1` that watches input files and pushes updates to the open page. |
| `--port <N>` | `auto` | Port for `--serve`. Default picks the first free port in `7272..7371`. |
| `--no-open` | `false` | Do not launch a browser. Print the file path or `http://127.0.0.1:<port>/` URL to stdout. |
| `--view <NAME>` | `gantt` | Default tab: `gantt`, `cube`, or `sankey`. |
| `--plan-state <NAME>` | derived | Override the plan-level state (see [Plan-Level State Derivation](#plan-level-state-derivation)). |

## Views

The rendered page has three tabs that share a single parsed dataset. Tab switching is instant (no reparse, no reload).

### 1. Swimlane Gantt (default)

Each row is an item: one row for the plan (level 0), one per task (level 1), and one per subtask (level 2) indented under its task. The x-axis is **state**, split into up to three column groups by level so each level's vocabulary renders in its own axis strip:

```
┌─────────────────────────┬─── LEVEL 0 — PLAN ───┬─── LEVEL 1 — TASK ──────────────────┬─── LEVEL 2 — SUBTASK ──────────────────┐
│ ◆  ·   rhei-run-tui     │               [active]│                                      │                                         │
├─────────────────────────┼───────────────────────┼──────────────────────────────────────┼─────────────────────────────────────────┤
│ ■  1   Create crate …   │                       │                [in_progress]         │                                         │
│ ·  1.1 Create crate …   │                       │                                      │                  [completed]            │
│ ·  1.2 Define RunEvent  │                       │                                      │            [in_progress]                │
│ ■  2   JournalSink …    │                       │       [pending]                      │                                         │
│ ·  2.1 Open log file    │                       │                                      │    [pending]                            │
│ …                       │                       │                                      │                                         │
└─────────────────────────┴───────────────────────┴──────────────────────────────────────┴─────────────────────────────────────────┘
```

Each level's pill only lands in its own column group. A gutter separates the groups. Empty groups (level has one state, or is collapsed) are omitted. When all three levels share an identical state vocabulary, the axis collapses to a single shared group.

Hovering a pill shows a tooltip with the item id, title, and state. The plan row is visually distinct (diamond bullet, stronger stroke, colored label).

### 2. Heatmap Cube

A dense grid: rows are tasks, columns are subtask slots (`.1`, `.2`, …). Each cell is colored by the subtask's state. A bordered cell to the left of each row shows the task's own state. The plan row is rendered as a full-width strip above the grid, colored by plan state.

Purpose: scan the whole plan at once. Good for answering "are there any red cells?" without reading labels.

### 3. Sankey Flow

Left column: one node per task. Right column: one node per state in the level-2 vocabulary. Ribbons flow from each task to the states its subtasks are in; ribbon thickness equals the count of subtasks in that state. The plan's aggregate state appears as a band above.

Purpose: spot bottlenecks — "most subtasks are stuck in `needs-review`" is immediately visible.

## Level-Grouped Axis Rules (Gantt)

Let `S₀`, `S₁`, `S₂` be the set of distinct states observed at the plan, task, and subtask levels respectively. The Gantt view computes:

- If `S₀ = S₁ = S₂`: render a single column group covering `S₀ ∪ S₁ ∪ S₂`. Every level's pills share the axis.
- Otherwise: render up to three column groups laid out left to right, one per non-empty level, with a 2-column-wide gutter between groups. Each group's columns list that level's states in the canonical ordering below. A level's pills only ever render in its own group.

Canonical state ordering (states not in this list sort after these, alphabetically):

```
draft → pending → in_progress → needs-review → review → prove → consolidate → completed → blocked/failed → cancelled → archived
```

This ordering governs column placement within every group.

## Plan-Level State Derivation

A `.rhei.md` plan does not carry an explicit `**State:**` at the top level (see [Plan Language Specification](rhei-plan-language.spec.md)). `rhei viz` derives a plan-level state from the top-level task states:

| Condition | Derived plan state |
|-----------|--------------------|
| All top-level tasks are `draft` | `draft` |
| All top-level tasks are terminal-success (`completed`) | `completed` |
| All top-level tasks are terminal (`completed`, `cancelled`, `archived`) and at least one is not `completed` | `archived` |
| Any top-level task is `in_progress`, `needs-review`, `review`, `prove`, `consolidate`, or `agent-review` | `active` |
| Otherwise | `pending` |

Derivation is done from the active state machine's terminal-state declarations — a plan using a custom state machine still derives correctly as long as the machine declares its terminal states.

`--plan-state <NAME>` overrides this derivation and forces a value (useful for demos or when the user wants to mark a plan `shipping` externally).

## Serving Modes

### Static (default)

`rhei viz [PATH]` writes a self-contained HTML file to a temp directory (`$TMPDIR/rhei-viz-<hash>.html`) with the plan data inlined as JSON, then launches the system browser against the `file://` URL. No background process remains after the browser opens.

The HTML contains zero external references: no `<script src>`, no `<link rel=stylesheet>`, no fonts loaded from the network. All CSS and JS are embedded. This means the page works offline, behind firewalls, and in review environments.

`--output <PATH>` redirects the file to a user-chosen location and suppresses the browser launch.

### Live (`--serve`)

`rhei viz --serve` starts a local HTTP server bound to `127.0.0.1`, serves the page at `/`, and opens the browser. The server:

- Watches every input `.rhei.md` file (and the enclosing workspace directory for file additions/removals) via the same `notify` crate already used by `rhei-tui`.
- On change: reparses the affected plan and pushes a `PlanUpdated` event over a long-poll or `EventSource` channel.
- Exits on `SIGINT` / Ctrl-C or when the last browser client disconnects for longer than 60 seconds.

The server is loopback-only. It does not accept remote connections and does not require authentication. Queries carry no credentials; the data served is already readable to the local user.

## Output Format

The rendered HTML has the following structure (abridged):

```html
<!doctype html>
<title>Rhei Viz — <plan title></title>
<style>/* inlined */</style>
<body>
  <header>plan selector, tab bar</header>
  <main>
    <svg id="svg-gantt">…</svg>
    <svg id="svg-cube">…</svg>
    <svg id="svg-sankey">…</svg>
  </main>
  <script>
    const DATA = { "<plan-key>": { title, state, tasks: [...] }, … };
    // rendering functions
  </script>
</body>
```

SVG is used instead of canvas so the output is scalable, copyable as an image, and crawlable by text-searching tools. No external JS framework.

## Data Shape

```ts
type Plan = {
  title: string;
  source: string;   // absolute path to the .rhei.md file
  state: string;    // level-0 (derived unless overridden)
  tasks: Task[];
};

type Task = {
  id: string;       // "1", "2", …
  title: string;
  state: string;    // level-1
  prior: string[];  // task ids this task depends on
  subtasks: Subtask[];
};

type Subtask = {
  id: string;       // "1.1", "1.2", …
  title: string;
  state: string;    // level-2
  prior: string[];  // subtask ids or task ids
};
```

The parser that produces this shape lives in `crates/rhei-core` (sharing code with `rhei next` / `rhei run`).

## CLI Integration

Add a `Viz` variant to the `Commands` enum in `crates/rhei-cli/src/main.rs`, with the clap fields above. The handler function `viz_command()` delegates to a new `crates/rhei-viz` crate that owns the HTML template, the static file embedding, and — gated behind the `serve` cargo feature — the HTTP server.

The `rhei-viz` crate depends on:
- `rhei-core` for plan parsing.
- `serde_json` for data inlining.
- `tiny_http` or `axum-minimal` (feature-gated) for `--serve`.
- `notify` (feature-gated) for file watching.

The non-`serve` build has no HTTP stack and no watcher; the default build stays small.

## Security Considerations

- The HTML file contains the full plan content (titles, contexts). Users writing plans with sensitive content should avoid `rhei viz --output` into shared directories.
- `--serve` binds to `127.0.0.1` only; it is not a multi-user server.
- Inlined plan data is injected as JSON via `serde_json::to_string()` and rendered into a `<script>` block. Titles and state names are escaped when used as text nodes (`textContent`) and when interpolated into tooltip HTML.

## Future Work

- **Click to open in editor.** A click on a task or subtask label opens the corresponding `.rhei.md` line range in `$EDITOR`.
- **Filter by state / level.** Click a state pill in the legend to dim items in other states.
- **DAG view.** A fourth tab that draws the `**Prior:**` graph, showing ready / blocked chains.
- **Diff mode.** `rhei viz --diff <git-ref>` highlights items whose state changed versus a prior commit.

These are explicitly out of scope for the first implementation and are listed here so reviewers can weigh them when evaluating the v1 design.
