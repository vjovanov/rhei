# FS-rhei-viz: Dashboard Visualization

Render plan-shape visualizations inside the browser dashboard that accompanies
`rhei run`. The visualization surface is read-only and uses the dashboard's
existing loopback server and `/snapshot` payload; there is no current standalone
`rhei viz` CLI command or `rhei-viz` crate. §FS-rhei-run-tui

For the plan grammar see the [Plan Language Specification](rhei-plan-language.spec.md).
For the state machine see the [States Specification](rhei-states.spec.md).

## Goals

1. **Make plan shape legible.** One page shows the plan, every task, every
   subtask, and each element's current state without reading raw markdown.
2. **Separate state vocabularies by level.** Plan, task, and subtask states may
   draw from different vocabularies; the visualization keeps them visually
   distinct so similarly named states at different levels are not conflated.
3. **Stay live with execution.** Views refresh through the same dashboard polling
   loop as slots, tasks, journal events, and links.
4. **Be self-contained.** The dashboard HTML contains its CSS and JavaScript
   inline, with no external scripts, stylesheets, fonts, or network assets.

## Non-Goals

- No standalone `rhei viz` command in the current CLI surface.
- No plan editing, editor-opening, diff mode, or mutation from the dashboard.
- No remote dashboard deployment; the dashboard remains loopback-only.

## 1. Dashboard Views

The dashboard exposes visualization tabs before the operational tabs:

1. **Gantt** — the default tab.
2. **Cube**.
3. **Sankey**.
4. **Tasks**.
5. **Slots**.
6. **Journal**.
7. **Links**.

Tab switching is local to the browser. All views share the same `/snapshot`
payload and must tolerate temporary plan reload failures by rendering the last
good snapshot already cached by the dashboard.

When a run finishes, the loopback server may exit with the CLI process, but the
operator must still have an inspectable final view. Dashboard-enabled runs write
a frozen self-contained HTML artifact under `runtime/` and print its path after
capturing the final `/snapshot` payload.

### 1.1. Swimlane Gantt

Each row is one item: a plan row, one row per top-level task, and one row per
child task indented beneath its parent. The x-axis is state, split into level
groups so level-0, level-1, and level-2 state vocabularies render in separate
axis strips unless all levels share the same vocabulary.

Each item places exactly one state pill in the group for its level. The plan row
is visually distinct from task rows and uses the derived plan state.

### 1.2. Heatmap Cube

Rows are top-level tasks. A left cell shows each task's own state;
descendant-state cells are laid out by root-relative descendant slot (`.1`,
`.2`, `.db`, `.auth.model`, ...). The derived plan state is shown as a
full-width strip above the grid.

This view favors density over labels so operators can quickly scan for failed,
blocked, review, or active work.

### 1.3. Sankey Flow

Left nodes are top-level tasks. Right nodes are descendant states. Ribbons flow
from each task to the states its descendants are in; ribbon thickness equals the
descendant count in that state. The derived plan state appears as a band above
the flow.

Plans without descendants render a useful empty state instead of a blank chart.

## 2. Level-Grouped Axis Rules

Let `S0`, `S1`, and `S2` be the distinct states observed at the plan, top-level
task, and child-task levels respectively.

- If `S0 = S1 = S2`, render one shared axis group.
- Otherwise, render one group per non-empty level from left to right.
- A pill may only render in the group for its own level.
- States within a group use this canonical order, with unknown states sorted
  alphabetically after known states:

```text
draft -> pending -> in_progress -> in-progress -> needs-review -> review ->
prove -> consolidate -> fix -> agent-review -> agent-review-fix ->
human-review -> active -> completed -> blocked -> failed -> cancelled ->
archived
```

## 3. Plan-State Derivation

A `.rhei.md` plan does not carry an explicit top-level state. The dashboard
derives `plan_state` from top-level tasks only:

| Condition | Derived plan state |
| --- | --- |
| All top-level tasks are `draft` | `draft` |
| All top-level tasks are `completed` | `completed` |
| All top-level tasks are terminal and at least one is not `completed` | `archived` |
| Any top-level task is currently assigned to a running slot | `active` |
| Any top-level task is active-like | `active` |
| Otherwise | `pending` |

The dashboard treats `completed`, `cancelled`, `archived`, and `failed` as
terminal for this derivation. Active-like states are `in_progress`,
`in-progress`, `needs-review`, `review`, `prove`, `consolidate`, and
`agent-review`, and `agent-review-fix`.

Custom non-terminal states that are not in the built-in color map use stable
fallback colors derived from the state name rather than the muted terminal color,
so a project-specific active state is visually distinguishable from archived or
cancelled work.

## 4. Dashboard Data

`/snapshot` includes:

```ts
type Snapshot = {
  plan_title?: string;
  plan_state?: string;
  tasks: TaskRow[];
  // plus existing run, slot, journal, ready/deferred, and link fields
};
```

Visualization views use the existing flattened task fields: `id`, `title`,
`parent`, `depth`, `state`, and `prior`. Root tasks are `depth == 1` or have no
`parent`; child tasks are rows with a parent.

Header progress is based on root task execution progress, not every flattened
child row. The Tasks tab may show all flattened rows, but scheduling labels and
filters such as running, ready, deferred, and blocked apply to runnable root
tasks; child rows are labeled as plan-shape child state. Tab badges use counts,
while derived state appears in the header, legend, or visualization body.

## Future Work

- Click to open a task in an editor.
- Filter or dim by state and level.
- DAG/dependency graph view.
- Diff visualization against another snapshot or git ref.
