# FS-rhei-viz: Flow Visualization

The **Flow view** is Rhei's primary visualization surface: one page that answers
the operator's two standing questions — *what is my Rhei doing, and why* — for a
plan or a Directory Workspace of plans. It pairs a navigable view of plan shape
with a surroundings inspector for any node and the resolved state machine the
plan runs under, and it leads with whatever is working right now.

The same renderer serves two modes from one model: a **dynamic** surface that
updates live during `rhei run`, and a byte-identical **static** artifact that can
be frozen at the end of a run or generated offline from a plan. A saved page is
indistinguishable from the live one except that it stops updating.

The visual language — monospace typography, calm desaturated color, near-zero
motion, the console-first feel shared with the terminal — is governed by
§FS-rhei-viz-ux. This spec defines *which* views exist and *what* data they show.
For the plan grammar see the [Plan Language Specification](rhei-plan-language.spec.md);
for the state machine see the [States Specification](rhei-states.spec.md).

The current reference implementation is the `xtask` static renderer
(`cargo xtask examples viz <name>`), which dogfoods this surface ahead of the
shipped command; the canonical design fixtures are `examples/inflight-dashboard`
and `examples/disjoint-tracks`.

## Goals

1. **One page, the whole plan.** The resting view shows the plan, every task, and
   every subtask, each marked with its current state — done, live, blocked,
   gated, failed, retired, or idle — without reading raw markdown and without a
   click.
2. **"Why," in the fewest clicks.** Any node — not only the running ones — can be
   entered to see its surroundings: what it depends on, what it unblocks, where it
   sits in its state machine and where it can go next, the prompt it runs, and its
   artifacts. Walking from a node to a neighbor and back costs one keystroke
   §GOAL-rhei-outcomes.
3. **Show the machine, not just the plan.** The resolved state machine is drawn as
   a graph — one per disjoint workflow — so the shape of the process is legible
   alongside the shape of the work.
4. **Lead with live work.** The view surfaces tasks that are running now, and a
   live task exposes its streaming agent output and a way to intervene.
5. **Dynamic and static parity.** The live surface and a frozen page render from
   one model with one layout and one behavior; the only difference is liveness.
6. **Self-contained and calm.** The page carries its CSS and JavaScript inline,
   with no external scripts, stylesheets, fonts, or network assets, and it obeys
   the console-first language of §FS-rhei-viz-ux.

## Non-Goals

- No plan editing, mutation, transitioning, or diff mode from the surface. It is
  read-only with respect to plan state: open-in-editor spawns the operator's
  editor (§11) and intervene messages a running agent (§5), but neither writes or
  advances the plan.
- No remote deployment. The live surface is loopback-only; the static surface is
  a local file.
- No redefinition of the look-and-feel; that is §FS-rhei-viz-ux.
- The dense chart overviews (Gantt, Cube, Sankey) are supplementary scanning aids
  (§12), not the primary surface.

## 1. The Flow Surface

The page is a single screen with five regions, top to bottom:

- **Header** — the plan title, a plan/workspace selector when more than one plan
  is present, and a one-line state-machine summary.
- **Strip** — a List/Graph mode toggle, summary counts, and the glyph legend.
- **Running-now panel** — chips for tasks in a live state; hidden when none are
  (§5).
- **Flow** — two panes. The left pane is the plan in **List** mode (§2) or
  **Graph** mode (§3); the right pane is the **Surroundings** inspector for the
  selected node (§4). In list mode the outline is a fixed column and surroundings
  grows to the right edge; in graph mode the DAG fills the left and surroundings
  is a fixed column. Below ~1060px the panes stack.
- **Machines** — the resolved state machine drawn as one graph per disjoint
  workflow, with a clickable legend and a state-detail panel (§6).

The surface is keyboard-first and mouse-optional, per §FS-rhei-viz-ux §7:
`j`/`k` or arrows move the selection through the plan in source order, `g` and
`l` switch to Graph and List mode, and the selection drives both the surroundings
inspector and the machine-detail panel together. All content is selectable and
copy-friendly.

### 1.1. State Category and Glyph

Every state is reduced to one of seven **categories**, derived from the resolved
machine's flags first and the state name second, so that custom vocabularies
classify correctly. The rows are evaluated top to bottom and the first match
wins, so the `live` catch-all only claims a state no earlier row matched:

| Category | Glyph | Meaning | Derivation |
| --- | --- | --- | --- |
| `done` | `✓` | completed | state `completed` |
| `blocked` | `⊘` | attention | state `blocked` |
| `failed` | `✗` | attention | state `failed` |
| `gate` | `⏸` | awaiting a gate | machine `gating`, or `human-review` |
| `retired` | `⊝` | terminal, not done | `cancelled`, `archived`, other terminal |
| `idle` | `·` | not started | `draft`, `pending`, or any profile's `initial` |
| `live` | `●` | working now | any non-terminal state not matched above |

Category, not raw state, drives the calm whole-line coloring of list rows and the
fill of graph nodes, so the eye reads status at a glance. `live` is the single
place motion is allowed: a spinner on list rows and marching-ants on graph nodes,
both stilled to a static dot under `prefers-reduced-motion`, per §FS-rhei-viz-ux
§4. State pills always carry the exact state text and its shared calm color from
§FS-rhei-viz-ux §3.2; color is never the only signal.

During a dynamic run, `task_runtime[id].in_slot` is an overlay on top of the
state-derived category: any task assigned to a live slot is shown as `live`, even
if its persisted implementation state is otherwise `idle` such as `pending`.
For an active agent slot, `task_runtime[id].template_context` carries the
invocation's concrete target/model/agent values so prompt and artifact previews
match the running process.

### 1.2. Summary and Legend

The strip shows live counts — total tasks, and how many are live, blocked, gate,
done, and failed — computed over top-level tasks, and a legend mapping each glyph
to its category. Counts and legend update in place on refresh (§7).

## 2. List Mode (Outline)

The left pane renders the plan as an indented outline: each top-level task
followed by its descendants, indented by id depth. Each row carries the
category glyph (or live spinner), the node id, the title, a left-aligned state
pill, and — for nodes with children — a subtree progress count (`done/total ✓`).
Clicking a row, or moving the keyboard selection onto it, selects the node and
populates surroundings. List mode is the default; it is the calmest overview and
the resting state of the surface.

## 3. Graph Mode (Dependency DAG)

The left pane renders the **prerequisite** graph over top-level tasks: nodes are
laid out in dependency layers by longest path over `**Prior:**` edges among
top-level tasks, with curved edges from each prior to its dependent. Node fill
follows the state category (§1.1); live nodes animate, stilled under reduced
motion. Clicking a node selects it. Graph mode answers "what unblocks what" at the
plan level; per-node prerequisite detail across all levels lives in the
surroundings inspector (§4).

## 4. Surroundings Inspector

The right pane is the inspector for the selected node — the heart of "why." For
the selected task or subtask it shows, top to bottom:

- **Head** — glyph, id, title, the state pill, and flags (`initial`/`terminal`/
  `gating` from the machine, plus `root task` or `depth N`), followed by the
  state's description.
- **Dependencies** — two columns: **depends on (Prior)** with each prerequisite as
  a chip marked satisfied when terminal, and **unblocks** with the nodes waiting
  on this one. Unresolved external priors render as flat chips. A "waiting on"
  line names any prior not yet terminal.
- **Came from** — the incoming transitions: which states this state can be entered
  from, with guard conditions, plus a `(from any · *)` marker when a wildcard rule
  applies; an initial state says so.
- **Next state** — the outgoing transitions: the states this one may move to, with
  guard conditions and a `(from *)` marker on wildcard-derived edges.
- **Prompt** — the state's agent instructions **instantiated for this node**:
  scalar template variables (`{task_id}`, `{task_title}`, `{visit_count}`,
  `{visits}`, `{model}`, …) resolved inline and highlighted, and
  `{input/output.<name>.path}` rendered as artifact links. Shown only when the
  state declares instructions.
- **Intervene** — for `live` nodes only, the streaming agent terminal and message
  composer (§5).
- **Children** — descendants with their glyphs, ids, titles, and states, and a
  `done/total ✓` header; shown only for nodes with children.
- **Artifacts** — the state's input (`in ◂`) and output (`out ▸`) contracts as
  links, with `{task_id}` and visit-count templates resolved for this node and
  optional artifacts marked.

Every chip in came-from, next-state, and the machine legend is clickable: a
transition chip highlights the target state across the machine graphs (§6) while
keeping the task in context; a dependency chip selects that neighbor. Clicking
across the inspector is how an operator walks the plan and the machine without
losing their place. Content swaps obey the single ≤150ms opacity fade of
§FS-rhei-viz-ux §4.

## 5. Running Execution View

The view leads with live work. The **running-now panel** shows one chip per
top-level task in the `live` category — id, short title, state pill, spinner —
and is hidden entirely when nothing is live. On load the surface auto-selects the
first live node so the resting view opens on what is working right now. Selecting
a chip selects its node.

When the selected node is `live`, the surroundings inspector adds an **intervene**
block: the agent's live output rendered as a real terminal — dark in any theme,
scrollback, ANSI color — that appends new lines in place from the run's
`AgentOutput` stream §FS-rhei-run-tui.1.2, plus a composer to send a message to
the agent working that task. The block also surfaces the slot's latest invocation
cost, input/output and cached token counts, accounting coverage, and elapsed time
§FS-rhei-cost-accounting.

In the static surface (§7.2) the terminal shows a representative transcript so the
layout has realistic shape, and the composer is inert (messages queue locally and
are not delivered). Intervene messages a running agent; it never transitions or
edits the plan.

## 6. State-Machine Graphs

The machine panel draws the resolved state machine as a graph so the workflow is
legible alongside the work. Disjoint workflows render as **separate graphs**: the
machine is split into connected components over non-wildcard transitions, and each
component is drawn on its own.

- **Multi-state components** render first, ordered by canonical state order (§10);
  a machine with more than one is labeled as `N disjoint` tracks, matching node
  kinds routed through `profiles`/`node_policy` (e.g. a feature, a bugfix, and a
  research track).
- **Isolated** single states render together.
- **Terminal** states reachable only via `from: "*"` wildcards render as a final
  graph labeled "reachable from any state (wildcard)," rather than gluing the
  tracks together through the escape edges.

Each graph uses a layered layout by longest path. Cycles — counted review/fix
loops, rework and reopen edges — are tolerated: a back edge (to a node still on
the DFS stack) is excluded from layering and drawn as a dashed loop with a
distinct arrowhead, so a `review ⇄ fix` loop reads as a loop. Each node is a
state pill colored by §10, carrying its flags in a tooltip. A legend lists every
state with its description and doubles as the prompt index.

Clicking a state — in a graph or the legend — opens the **state-detail** panel:
its flags (`initial`/`terminal`/`gating`, and `counted ×N` when the state
declares `visits: N`), description, and the raw prompt template for the state.
Selecting a task highlights its current state across every graph and shows the
prompt *instantiated* for that task in the surroundings inspector (§4), so the
operator sees both the template and its resolution. The plan's overview prose, if
present, is shown above the graphs as "what this Rhei is doing."

## 7. Dynamic and Static Rendering

One renderer and one model (§8) drive two modes.

### 7.1. Dynamic (live during `rhei run`)

The live surface is served on the dashboard's loopback server alongside `rhei run`
§FS-rhei-run-tui.1.6 and refreshes through the same polling loop as the rest of
the dashboard. Liveness is shown by content changing in place, never by movement,
per §FS-rhei-viz-ux §4: a poll updates row text, pills, running-now chips, and the
agent terminal where they sit, with no layout shift, stable ordering, and the
operator's scroll position and text selection preserved. The surface tolerates a
temporary plan-reload failure by rendering the last-good snapshot already cached.
Artifact and log links open through the open-in-editor route (§11). When the run
finishes and the loopback server exits, the surface writes a frozen,
self-contained HTML artifact under `runtime/` from the final snapshot, so the
operator keeps an inspectable view.

### 7.2. Static (offline or frozen)

The static surface is a single self-contained HTML file generated from a plan — or
a Directory Workspace, merging the `index.rhei.md` with any standalone plan files
— together with its resolved state machine, with the model embedded inline and no
live polling. Artifact and log links are **illustrative**: the files they point to
materialize under `runtime/` only during a live run. The agent terminal shows a
representative transcript and the intervene composer is inert (§5). This is what
`cargo xtask examples viz <name>` produces today and what the frozen run-end
artifact (§7.1) reuses; it is also the surface a future `rhei viz` command renders.

## 8. Data Contract

Both modes consume one model. The live `/snapshot` payload is a superset of the
embedded static bundle:

```ts
type Snapshot = {
  plan_title?: string;
  plan_state?: string;          // derived, §9
  about?: string;               // plan overview prose, shown above the machine
  accounting?: AccountingRunSummary;
  tasks: TaskRow[];             // id, title, parent, depth, state, visit_count?, prior
  machine: Machine;             // the resolved state machine, flattened (below)
  // plus existing run, slot, journal, ready/deferred, and link fields
};

type Machine = {
  name: string;
  states: MachineState[];
};

type MachineState = {
  name: string;
  description?: string;
  instructions?: string;        // the agent prompt template, variables unresolved
  visits?: number;              // counted-loop budget when declared
  initial: boolean;             // entry state of at least one profile (§FS-rhei-states; initiality is per-profile)
  terminal: boolean;
  gating: boolean;
  transitions: Transition[];    // explicit edges first, then applicable wildcards
  inputs: Artifact[];
  outputs: Artifact[];
  template_context?: TemplateContext;
  template_contexts?: TemplateContext[]; // authored fanout variants for static previews
};

type TemplateContext = {
  target?: string;
  target_slug?: string;
  model?: string;
  model_provider?: string;
  model_name?: string;
  agent?: string;
  agent_mode?: string;
};
type Transition = { to: string; condition?: string; wildcard: boolean };
type Artifact = { name: string; path: string; description?: string; optional: boolean };
```

Rules:

- **The host supplies the machine.** `rhei run` resolves the state machine and
  passes it to the surface; the static renderer resolves it from the plan's
  `**States:**` declaration (an override, then a matching sibling `states.yaml`,
  then the built-in default for `rhei`). The surface never parses plans or state
  YAML itself. §FS-rhei-states
- **Transitions are flattened per state.** Each non-terminal state carries its
  explicit outgoing edges plus any `from: "*"` wildcard edges that apply to it,
  each marked `wildcard`, so the inspector shows the real set of legal exits.
- **Initiality is per-profile.** A machine has no single initial state; each
  profile declares its own entry state, and a state may be the entry of one
  profile but not another §FS-rhei-states. The flattened `initial` flag is the
  union — true when a state is the entry of at least one profile — which is all
  the surface needs to mark track-entry states across disjoint graphs.
- **Templates resolve per node.** `{task_id}`, `{task_title}`, `{visit_count}`,
  `{visits}`, `{model}` and similar scalars, and `{input/output.<name>.path}`
  artifact references, are resolved against the selected node when rendering its
  prompt (§4) and artifact links. A live render substitutes the running task's
  real values, including the concrete target/model/agent from its slot. A static
  render resolves authored single-target/model states directly, preserves any
  `-N` visit suffix from the plan row as `visit_count`, and renders multi-target
  or multi-model fanout states once per authored context so artifact paths and
  prompt previews do not point at guessed values.
- **Compact rollups in `/snapshot`, detail elsewhere.** Each task row may carry
  compact direct and subtree accounting rollups; invocation-level detail is served
  from a separate loopback endpoint so polling stays light. §FS-rhei-cost-accounting

## 9. Plan-State Derivation

A `.rhei.md` plan carries no explicit top-level state. The surface derives
`plan_state` from top-level tasks only, using the resolved machine to classify
terminal states:

| Condition | Derived plan state |
| --- | --- |
| All top-level tasks are `draft` | `draft` |
| All top-level tasks are `completed` | `completed` |
| All top-level tasks are terminal and at least one is not `completed` | `archived` |
| Any top-level task is assigned to a running slot, or is active-like | `active` |
| Otherwise | `pending` |

Terminal states are those the machine flags terminal (built-ins: `completed`,
`cancelled`, `archived`, `failed`). Active-like means a non-terminal state that
is not `idle` (§1.1): not `draft`, not `pending`, and not a profile entry state.
A plan of `pending` tasks therefore derives `pending`, not `active`, under any
profile.

## 10. Level-Grouped State Vocabularies

Plan, task, and subtask levels may draw from different state vocabularies, and the
surface keeps them visually distinct so similarly named states at different levels
are not conflated. States sort by this canonical order, with unknown states sorted
alphabetically after known states:

```text
draft -> pending -> in_progress -> in-progress -> needs-review -> review ->
prove -> consolidate -> fix -> agent-review -> agent-review-fix ->
human-review -> active -> completed -> blocked -> failed -> cancelled ->
archived
```

Custom non-terminal states not in the built-in color map get a stable
name-derived color, desaturated to the same level as the calm palette so a
project-specific state never out-shouts a built-in attention state and is still
distinguishable from terminal work, per §FS-rhei-viz-ux §3.2.

## 11. Open-in-Editor Route

Artifact and log links resolve through a loopback `GET /open?path=<rel>` endpoint
rather than `file://` navigation, so a click opens the file in the operator's
editor on the same machine. The route:

- accepts only a workspace-relative `path`; absolute paths and paths that escape
  the workspace root are rejected with `400`;
- resolves the editor from `RHEI_EDITOR`, then `VISUAL`, then `EDITOR`, then a
  platform opener (`xdg-open`, `open`, or `cmd /C start`);
- launches the editor detached on the resolved absolute path and replies `204`
  without waiting; a launch failure replies `500`;
- remains loopback-only, like the rest of the dashboard server.

The endpoint spawns a local process but never reads, writes, or transitions plan
state, so the surface stays read-only with respect to the plan.

## 12. Supplementary Dense Views

For scanning many nodes at once, the surface may offer dense chart overviews
alongside the Flow view. They consume the same `/snapshot` data and the same
console-first language, and are secondary to the Flow view rather than the primary
surface:

- **Gantt** — one row per item (plan row, each top-level task, each child indented
  beneath its parent) against state axes split into per-level groups when level
  vocabularies differ (§10).
- **Cube** — a dense top-level-task by descendant-slot heatmap, with the derived
  plan state as a full-width strip; can switch from state coloring to a
  subtree-cost heatmap. §FS-rhei-cost-accounting
- **Sankey** — ribbons from each top-level task to its descendants' states, ribbon
  thickness equal to the descendant count, or to cost. §FS-rhei-cost-accounting

Plans without descendants render a useful monochrome empty state rather than a
blank chart, per §FS-rhei-viz-ux §7.

## Related Specifications

- [Console-First Visualization UX](rhei-viz-ux.spec.md) — the shared look-and-feel
  every surface follows. §FS-rhei-viz-ux
- [`rhei run` TUI and Transition Journal](rhei-run-tui.spec.md) — the terminal
  sibling, the loopback dashboard host, and the `AgentOutput` stream the live
  terminal renders. §FS-rhei-run-tui
- [Cost Accounting](rhei-cost-accounting.spec.md) — the accounting data the
  running view and cost overlays present. §FS-rhei-cost-accounting
- [States Specification](rhei-states.spec.md) — the state machine, profiles,
  artifact contracts, and counted loops the machine graphs and inspector render.

## Future Work

- Filter or dim the plan by state, level, or category.
- Diff visualization against another snapshot or git ref.
- Cross-plan navigation when a workspace bundles many plans.
- Wider intervene controls (pause, retry, redirect) beyond messaging.
