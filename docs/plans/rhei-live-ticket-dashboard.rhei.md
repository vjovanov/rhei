# Rhei: Live Ticket Dashboard
**States:** rhei-live-ticket-dashboard

## Context

Build a live browser dashboard for `rhei run` that shows GitHub tickets and
agent progress while the terminal TUI remains active. The dashboard is served
from the local `rhei run` process on `127.0.0.1`, and browser JavaScript
receives live updates from the same run-event stream that drives the TUI and
transition journal.

The dashboard is not a replacement for `rhei viz`. `rhei viz` visualizes plan
shape; this dashboard visualizes live execution and external queue items such
as GitHub issues and pull requests.

## Tasks

### Task 1: Define dashboard data contracts
**State:** completed

Design the Rust and JSON data structures for the live dashboard. Include a
run snapshot, agent slot records, ticket records, and incremental dashboard
events suitable for browser consumption.

#### Task 1.1: Define web-facing DTOs
**State:** draft

Create DTOs for `RunSnapshot`, `AgentSlot`, `Ticket`, and `DashboardEvent`.
Keep the DTOs separate from `RunEvent` so the browser contract can evolve
without exposing internal enum details.

#### Task 1.2: Specify ticket identity and linking fields
**State:** draft

Define stable ticket keys such as `issue:1234` and `pr:5678`. Include fields
for GitHub URL, title, labels, source kind, route hint, class slug, linked
Rhei task id, current task state, and latest action.

### Task 2: Add structured ticket artifacts
**State:** completed
**Prior:** Task 1

Extend the hourly human-intervention workflow so fetch tasks write a structured
ticket artifact in addition to the existing markdown inventory and
classification files.

#### Task 2.1: Add `runtime/tickets/items.json`
**State:** draft

Update fetch-state instructions so agents upsert ticket records into
`runtime/tickets/items.json`. Preserve markdown artifacts for human reading,
but make JSON the dashboard source of truth.

#### Task 2.2: Make ticket writes idempotent
**State:** draft

Document and implement merge behavior so repeated hourly sweeps update existing
ticket records by stable key instead of duplicating entries.

### Task 3: Implement the live dashboard sink
**State:** completed
**Prior:** Task 1

Add a sink that consumes `RunEvent`s, maintains dashboard state, and broadcasts
web-friendly events to browser clients.

#### Task 3.1: Build the dashboard state reducer
**State:** draft

Apply `RunStarted`, `PassStarted`, `SlotAssigned`, `AgentOutput`,
`SlotReleased`, and `RunFinished` to an in-memory `DashboardState`.
Keep recent output bounded per slot.

#### Task 3.2: Watch ticket artifact updates
**State:** draft

Watch `runtime/tickets/items.json` and reload it when it changes. Emit ticket
update events after successful parses and surface parse failures as dashboard
warnings without crashing the run.

### Task 4: Serve the live browser page
**State:** completed
**Prior:** Task 3

Start a local HTTP server from `rhei run` when the dashboard is enabled.
Bind only to loopback and expose a self-contained dashboard page.

#### Task 4.1: Add dashboard CLI flags
**State:** draft

Add `--dashboard` and `--no-dashboard` to `rhei run`. Default the dashboard on
when TUI mode is active, and keep it off for non-interactive CI output unless
the user explicitly requests it.

#### Task 4.2: Add snapshot and event endpoints
**State:** draft

Serve `/snapshot` as JSON and `/events` as Server-Sent Events. Use SSE for
one-way live updates from the run process to browser JavaScript.

#### Task 4.3: Serve the dashboard HTML
**State:** draft

Serve `/` as a self-contained HTML page with embedded CSS and JavaScript. The
page should fetch the initial snapshot, subscribe to `/events`, and update
without reloads. The visual style is shared with `rhei viz` — see Task 4.4.

#### Task 4.4: Match the `rhei viz` visual style
**State:** draft

Style the dashboard chrome to match `rhei viz` so a user moving between the
two views feels they belong to the same product. Concretely:

- Reuse the dark palette: `--bg #0b1020`, `--panel #131a2e`, `--panel-2 #1a2440`,
  `--ink #e7ecf5`, `--muted #8ea0c5`, `--line #263259`, `--accent #93c5fd`.
- Reuse the gradient header (`linear-gradient(90deg, #0b1020, #131a2e)`) with
  an `h1` title, a muted `sub` tagline, and right-aligned `.controls`.
- Reuse the tab bar pattern: `button.tab` with a transparent border and an
  accent-colored underline on `.active`, sitting on a `var(--panel)` strip.
- Reuse panel chrome: 1px `var(--line)` border, `border-radius: 10px`, panel
  background, system font stack (`-apple-system, BlinkMacSystemFont, "Segoe UI",
  Roboto, …`), 12–13px base size.
- Reuse the legend swatch style (`.legend .sw`) and the `STATE_COLOR` map
  defined in `xtask/assets/viz-template.html` for any state pills shown on
  slots or tickets, so a `completed` pill in the dashboard matches the same
  pill in `rhei viz`.
- Reuse the tooltip pattern (fixed-position `.tooltip` div, populated on
  `mousemove`).
- Where a visualization fits the data — for example, an SVG strip showing one
  pill per active agent slot keyed on the same state palette — prefer SVG over
  ad-hoc HTML so the look matches `rhei viz` panels.
- Keep the page self-contained: embedded CSS and JS only, no CDN scripts, no
  external fonts, no remote images.

The shared style assets should be sourced from `xtask/assets/viz-template.html`
or extracted into a small shared snippet so the two pages cannot drift.

### Task 5: Link the dashboard from the TUI
**State:** completed
**Prior:** Task 4

Surface the dashboard URL in the terminal TUI so users can discover the live
page while a run is active.

#### Task 5.1: Add a generic run-link event
**State:** draft

Add a `RunLink` or `DashboardStarted` event that carries a label and target
URL. Emit it after the server successfully binds.

#### Task 5.2: Render the URL in header and journal
**State:** draft

Show the dashboard URL in the TUI header and append a journal line when it
starts. Also print the URL in stdout mode when the dashboard is explicitly
enabled.

### Task 6: Build the ticket dashboard UI
**State:** completed
**Prior:** Task 4

Implement the browser UI for monitoring the run and ticket queue.

#### Task 6.1: Render active agent slots
**State:** draft

Show one compact slot per active worker with task id, title, current state,
agent, elapsed time, and recent output.

#### Task 6.2: Render the ticket queue
**State:** draft

Show tickets in a dense table with number, title, labels, route hint, class,
linked task, task state, and latest action. Let users filter by route, label,
state, and text.

#### Task 6.3: Render details for selected items
**State:** draft

When a user selects a ticket or agent slot, show linked logs, recent output,
classification evidence, and GitHub links in a details pane.

### Task 7: Correlate tickets with Rhei tasks
**State:** completed
**Prior:** Task 2, Task 6

Connect external GitHub tickets to generated Rhei child tasks so the dashboard
can show which agent is handling each queue item.

#### Task 7.1: Prefer explicit task links from JSON
**State:** draft

Use `linked_task_id` from `runtime/tickets/items.json` whenever present.
Display tickets without a link in an unassigned section.

#### Task 7.2: Add fallback content scanning
**State:** draft

When no explicit link exists, scan parsed task content for GitHub issue and PR
URLs. Use this only as a fallback and avoid mutating plan files from the
dashboard.

### Task 8: Verify and document the feature
**State:** completed
**Prior:** Task 5, Task 7

Add tests and documentation for the live dashboard.

#### Task 8.1: Test event reduction and ticket reloads
**State:** draft

Add unit tests for the dashboard state reducer and ticket JSON loading,
including malformed JSON and bounded output retention.

#### Task 8.2: Test HTTP endpoints
**State:** draft

Add integration tests for `/snapshot`, `/events`, and server shutdown behavior.
Verify the server binds to `127.0.0.1` only.

#### Task 8.3: Update user documentation
**State:** draft

Document `rhei run --tui --dashboard`, the ticket JSON schema, the loopback
security model, and how the TUI links to the browser dashboard.
