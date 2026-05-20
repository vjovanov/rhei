pub(super) const DASHBOARD_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rhei Run Dashboard</title>
<style>
  :root {
    --bg: #0b1020;
    --panel: #131a2e;
    --panel-2: #1a2440;
    --ink: #e7ecf5;
    --muted: #8ea0c5;
    --line: #263259;
    --accent: #93c5fd;
    --warn: #f59e0b;
    --error: #ef4444;
    --ok: #10b981;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: var(--bg); color: var(--ink);
    font: 13px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
  header { padding: 14px 20px; background: linear-gradient(90deg, #0b1020, #131a2e);
    border-bottom: 1px solid var(--line); display: grid;
    grid-template-columns: 1fr auto; gap: 12px 16px; align-items: baseline; }
  header h1 { margin: 0; font-size: 17px; font-weight: 600; letter-spacing: .2px; }
  header .ws { grid-column: 1 / -1; color: var(--muted); font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  header .ws:hover { color: var(--ink); }
  .status-pill { color: var(--muted); font-size: 12px; }
  .status-pill .seg { padding: 2px 8px; border-radius: 999px; background: #1a2440;
    margin-right: 6px; }
  .status-pill .seg.ok { background: #062e21; color: #6ee7b7; }
  .status-pill .seg.warn { background: #3f2d05; color: #fde68a; }
  .status-pill .seg.busy { background: #102a4f; color: #93c5fd; }
  .banner { display: none; padding: 8px 20px; font-size: 12px; }
  .banner.show { display: block; }
  .banner.error { background: #3a0d12; color: #fecaca; border-bottom: 1px solid #7f1d1d; }
  .banner.done  { background: #062e21; color: #6ee7b7; border-bottom: 1px solid #047857; }
  .tabs { display: flex; gap: 4px; padding: 0 20px; border-bottom: 1px solid var(--line);
    background: var(--panel); position: sticky; top: 0; z-index: 10; overflow-x: auto;
    scrollbar-width: thin; }
  button.tab { background: transparent; color: var(--muted); border: 0;
    border-bottom: 2px solid transparent; border-radius: 0; padding: 10px 14px;
    font: inherit; cursor: pointer; white-space: nowrap; flex: 0 0 auto; }
  button.tab.active { color: var(--ink); border-bottom-color: var(--accent); }
  button.tab .count { color: var(--muted); margin-left: 6px; font-size: 11px; }
  main { padding: 18px 20px; }
  .legend { display: flex; gap: 14px; flex-wrap: wrap; color: var(--muted);
    font-size: 12px; margin-bottom: 12px; }
  .legend .sw { display: inline-block; width: 10px; height: 10px; border-radius: 3px;
    vertical-align: -1px; margin-right: 6px; }
  .view { display: none; }
  .view.active { display: block; }
  .caption { color: var(--muted); font-size: 12px; margin: 6px 2px 14px; }
  .panel { background: var(--panel); border: 1px solid var(--line); border-radius: 10px;
    overflow: hidden; }
  .panel + .panel { margin-top: 10px; }
  .viz-panel { overflow: auto; padding: 12px; }
  .viz-svg { display: block; min-width: 100%; }
  .viz-axis { fill: var(--muted); font: 11px ui-monospace, SFMono-Regular, Menlo, monospace; }
  .viz-label { fill: var(--ink); font-size: 12px; }
  .viz-muted { fill: var(--muted); font-size: 11px; }
  .viz-line { stroke: var(--line); stroke-width: 1; }
  .viz-pill-text { fill: #0b1020; font-size: 11px; font-weight: 700; }
  .viz-node-text { fill: var(--ink); font-size: 12px; font-weight: 600; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; color: var(--muted); font-weight: 500; font-size: 11px;
    padding: 8px 12px; border-bottom: 1px solid var(--line); background: #0f1530;
    text-transform: uppercase; letter-spacing: .04em; }
  td { border-bottom: 1px solid var(--line); padding: 8px 12px; vertical-align: top;
    color: var(--ink); }
  tr:last-child td { border-bottom: 0; }
  tr.row-running td { background: rgba(59, 130, 246, 0.08); }
  tr.row-done td { color: var(--muted); }
  .mono { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--muted); }
  .chip { display: inline-block; padding: 2px 8px; border-radius: 999px; font-size: 11px;
    font-weight: 600; color: #0b1020; background: #475569; }
  .chip.outline { background: transparent; border: 1px solid currentColor; color: inherit; }
  .chip.muted { background: #1a2440; color: var(--muted); }
  .chip.ok { background: var(--ok); }
  .chip.warn { background: var(--warn); }
  .chip.busy { background: #3b82f6; color: #0b1020; }
  .filters { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 12px; }
  .filters button { background: #1a2440; color: var(--muted); border: 0; padding: 5px 10px;
    border-radius: 999px; font: inherit; font-size: 12px; cursor: pointer; }
  .filters button.active { background: var(--accent); color: #0b1020; }
  .slot-card { padding: 10px 12px; }
  .slot-card .head { font-weight: 600; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .slot-card .meta { color: var(--muted); font-size: 12px; margin-top: 2px;
    display: flex; gap: 12px; flex-wrap: wrap; }
  .slot-card .meta a { color: var(--accent); }
  .slot-card .out { margin-top: 8px; max-height: 180px; overflow: auto;
    background: #0a0f23; border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }
  .slot-card .out .line { display: flex; gap: 8px; align-items: baseline;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11.5px; }
  .slot-card .out .ts { color: var(--muted); width: 56px; flex: 0 0 auto; }
  .slot-card .out .stream { width: 50px; flex: 0 0 auto; font-weight: 600; }
  .slot-card .out .stream.stdout { color: var(--accent); }
  .slot-card .out .stream.stderr { color: var(--warn); }
  .slot-card .out .text { white-space: pre-wrap; word-break: break-word; flex: 1; }
  .slot-card .out .repeat { color: var(--muted); }
  .slot-card.idle { opacity: .55; }
  .toggle { display: inline-flex; gap: 6px; align-items: center; color: var(--muted);
    font-size: 12px; cursor: pointer; user-select: none; }
  .journal-line { display: flex; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--line);
    font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }
  .journal-line:last-child { border-bottom: 0; }
  .journal-line .ts { color: var(--muted); width: 80px; flex: 0 0 auto; }
  .journal-line .lvl { width: 56px; flex: 0 0 auto; font-weight: 600; }
  .journal-line.info .lvl { color: var(--muted); }
  .journal-line.warn .lvl { color: var(--warn); }
  .journal-line.error .lvl { color: var(--error); }
  .journal-line .text { white-space: pre-wrap; word-break: break-word; flex: 1; color: var(--ink); }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .empty { padding: 14px; color: var(--muted); font-size: 12px; }
</style>
</head>
<body>
<header>
  <h1 id="plan-title">Rhei Run Dashboard</h1>
  <div class="status-pill" id="status"><span class="seg">connecting…</span></div>
  <div class="ws" id="workspace" title="click to copy">live execution</div>
</header>
<div class="banner" id="banner"></div>
<div class="tabs">
  <button class="tab active" data-view="gantt">Gantt <span class="count" id="count-gantt"></span></button>
  <button class="tab" data-view="cube">Cube <span class="count" id="count-cube"></span></button>
  <button class="tab" data-view="sankey">Sankey <span class="count" id="count-sankey"></span></button>
  <button class="tab" data-view="tasks">Tasks <span class="count" id="count-tasks"></span></button>
  <button class="tab" data-view="slots">Slots <span class="count" id="count-slots"></span></button>
  <button class="tab" data-view="cost">Cost <span class="count" id="count-cost"></span></button>
  <button class="tab" data-view="journal">Journal <span class="count" id="count-journal"></span></button>
  <button class="tab" data-view="links">Links <span class="count" id="count-links"></span></button>
</div>
<main>
  <div class="legend" id="legend"></div>

  <div class="view active" id="view-gantt">
    <div class="caption">Plan shape by level. State axes are separated when level vocabularies differ.</div>
    <div class="panel viz-panel" id="gantt-panel"></div>
  </div>

  <div class="view" id="view-cube">
    <div class="caption">Dense task-by-descendant-state heatmap for scanning the whole plan.</div>
    <div class="panel viz-panel" id="cube-panel"></div>
  </div>

  <div class="view" id="view-sankey">
    <div class="caption">Descendant-state flow by top-level task. Ribbon thickness is descendant count.</div>
    <div class="panel viz-panel" id="sankey-panel"></div>
  </div>

  <div class="view" id="view-tasks">
    <div class="caption">Every task in the plan. Re-read on each refresh.</div>
    <div class="filters" id="task-filters"></div>
    <div class="panel">
      <table>
        <thead>
          <tr>
            <th>ID</th><th>Title</th><th>State</th><th>Assignee</th><th>Prior</th><th>Now</th>
          </tr>
        </thead>
        <tbody id="tasks"></tbody>
      </table>
    </div>
  </div>

  <div class="view" id="view-slots">
    <div class="caption">One card per worker. Idle workers are dimmed.</div>
    <label class="toggle"><input type="checkbox" id="hide-stderr"> Hide stderr lines</label>
    <div id="slots-detail"></div>
  </div>

  <div class="view" id="view-cost">
    <div class="caption">Token and cost accounting reported by agent invocations.</div>
    <div class="panel" id="cost-panel"></div>
  </div>

  <div class="view" id="view-journal">
    <div class="caption">Run-event lines, oldest first. Auto-scrolls to bottom while you're at the bottom.</div>
    <div class="panel" id="journal-panel" style="max-height: 600px; overflow: auto;">
      <div id="journal"></div>
    </div>
  </div>

  <div class="view" id="view-links">
    <div class="caption">Workspace shortcuts plus links emitted by the run process.</div>
    <div class="panel"><table><tbody id="links"></tbody></table></div>
  </div>
</main>
<script>
const STATE_ORDER = [
  "draft", "pending", "in_progress", "in-progress", "needs-review",
  "review", "prove", "consolidate", "fix", "agent-review",
  "agent-review-fix", "human-review", "active", "completed", "blocked",
  "failed", "cancelled", "archived",
];
const STATE_COLOR = {
  "draft":            "#64748b",
  "pending":          "#94a3b8",
  "in-progress":      "#3b82f6",
  "in_progress":      "#3b82f6",
  "needs-review":     "#f59e0b",
  "review":           "#a855f7",
  "prove":            "#06b6d4",
  "consolidate":      "#14b8a6",
  "fix":              "#f97316",
  "agent-review":     "#8b5cf6",
  "agent-review-fix": "#ec4899",
  "human-review":     "#22c55e",
  "active":           "#38bdf8",
  "completed":        "#10b981",
  "blocked":          "#ef4444",
  "failed":           "#ef4444",
  "cancelled":        "#475569",
  "archived":         "#334155",
};
const FALLBACK_STATE_COLORS = [
  "#06b6d4", "#84cc16", "#eab308", "#f97316", "#ec4899",
  "#8b5cf6", "#14b8a6", "#f43f5e", "#0ea5e9", "#a3e635",
];
function fallbackStateColor(s) {
  let hash = 0;
  for (const ch of String(s || "")) hash = ((hash * 31) + ch.charCodeAt(0)) >>> 0;
  return FALLBACK_STATE_COLORS[hash % FALLBACK_STATE_COLORS.length];
}
function stateColor(s) { return STATE_COLOR[s] || fallbackStateColor(s); }
function stateIndex(s) {
  const i = STATE_ORDER.indexOf(s);
  return i >= 0 ? i : STATE_ORDER.length;
}
function escapeHtml(s) {
  return String(s == null ? "" : s).replace(/[&<>"']/g, c =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
function fmtDuration(ms) {
  if (ms == null || ms < 0) return "";
  if (ms < 1000) return ms + "ms";
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + "s";
  const m = Math.floor(s / 60);
  const rem = s % 60;
  if (m < 60) return rem ? `${m}m${rem}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return remM ? `${h}h${remM}m` : `${h}h`;
}
function fmtClock(ms) {
  if (!ms) return "";
  const d = new Date(Number(ms));
  const pad = n => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function fmtTokens(v) {
  if (v == null) return "—";
  if (v >= 1000000) return (v / 1000000).toFixed(v >= 10000000 ? 0 : 1) + "M";
  if (v >= 1000) return (v / 1000).toFixed(v >= 10000 ? 0 : 1) + "k";
  return String(v);
}

function fmtCostMicro(v, currency) {
  if (v == null) return "—";
  const amount = v / 1000000;
  if (!currency || currency === "USD") return "$" + amount.toFixed(2);
  return amount.toFixed(2) + " " + currency;
}

function fmtSummaryCost(summary) {
  return fmtCostMicro(summary && (summary.cost_micro ?? summary.priced_cost_micro), summary && summary.currency);
}

function dimValue(summary) {
  return summary && summary.value != null ? summary.value : null;
}

let UI = {
  taskFilter: "all",
  hideStderr: false,
};

function setBanner(kind, text) {
  const el = document.getElementById("banner");
  if (!text) { el.classList.remove("show"); return; }
  el.className = "banner show " + kind;
  el.textContent = text;
}

function renderHeader(data) {
  const title = data.plan_title || "Rhei Run Dashboard";
  document.getElementById("plan-title").textContent = title;
  document.title = data.plan_title ? `Rhei: ${title}` : "Rhei Run Dashboard";
  const ws = document.getElementById("workspace");
  ws.textContent = data.workspace || "";
  ws.onclick = () => navigator.clipboard?.writeText(data.workspace || "");

  const slots = data.slots || [];
  const running = slots.filter(s => s.active).length;
  const deferred = (data.deferred || []).length;
  const rootTasks = (data.tasks || []).filter(isRunnableTask);
  const total = data.total_tasks || rootTasks.length || (data.tasks || []).length;
  const done = rootTasks.filter(t => isTerminal(t.state)).length;
  const elapsedMs = data.started_at_ms ? (data.updated_at_ms - data.started_at_ms) : 0;
  const segs = [
    `<span class="seg">Pass ${data.pass || 0}</span>`,
    `<span class="seg busy">${running} running</span>`,
  ];
  if (deferred > 0) segs.push(`<span class="seg warn">${deferred} deferred</span>`);
  segs.push(`<span class="seg ${done === total && total > 0 ? "ok" : ""}">${done}/${total} done</span>`);
  if (data.finished) {
    segs.push(`<span class="seg ok">finished</span>`);
  } else {
    segs.push(`<span class="seg">${fmtDuration(elapsedMs)} elapsed</span>`);
  }
  if (data.accounting) {
    segs.push(`<span class="seg">Cost ${fmtCostMicro(data.accounting.cost_micro ?? data.accounting.priced_cost_micro, data.accounting.currency)}</span>`);
    segs.push(`<span class="seg">In ${fmtTokens(dimValue(data.accounting.input_total))}</span>`);
    segs.push(`<span class="seg">Out ${fmtTokens(dimValue(data.accounting.output_total))}</span>`);
    segs.push(`<span class="seg">Coverage ${escapeHtml(data.accounting.coverage || "unknown")}</span>`);
  }
  document.getElementById("status").innerHTML = segs.join("");
}

function isTerminal(state) {
  return state === "completed" || state === "cancelled" || state === "archived" || state === "failed";
}

function renderLegend(data) {
  const box = document.getElementById("legend");
  const states = new Set();
  if (data.plan_state) states.add(data.plan_state);
  for (const t of data.tasks || []) if (t.state) states.add(t.state);
  for (const s of data.slots || []) if (s.state) states.add(s.state);
  const ordered = [...states].sort((a, b) => stateIndex(a) - stateIndex(b));
  box.innerHTML = ordered.map(s =>
    `<span><span class="sw" style="background:${stateColor(s)}"></span>${escapeHtml(s)}</span>`
  ).join("") || `<span class="mono">(no states yet)</span>`;
}

function setCounts(data) {
  const rowCount = (data.tasks || []).length;
  const childCount = (data.tasks || []).filter(t => isChildTask(t)).length;
  document.getElementById("count-gantt").textContent = rowCount ? rowCount + 1 : "";
  document.getElementById("count-cube").textContent = childCount || "";
  document.getElementById("count-sankey").textContent = childCount || "";
  document.getElementById("count-tasks").textContent = rowCount || "";
  const running = (data.slots || []).filter(s => s.active).length;
  document.getElementById("count-slots").textContent = running ? `${running}/${(data.slots || []).length}` : "";
  document.getElementById("count-cost").textContent = data.accounting ? ((data.accounting_by_state || []).length || data.accounting.invocation_count || "") : "";
  document.getElementById("count-journal").textContent = (data.recent || []).length || "";
  const links = (data.auto_links || []).length + (data.links || []).length;
  document.getElementById("count-links").textContent = links || "";
}

function truncateLabel(s, n) {
  s = String(s == null ? "" : s);
  return s.length > n ? s.slice(0, Math.max(0, n - 1)) + "…" : s;
}

function uniqueSorted(values) {
  return [...new Set(values.filter(Boolean))].sort((a, b) => {
    const ai = stateIndex(a);
    const bi = stateIndex(b);
    if (ai !== bi) return ai - bi;
    return String(a).localeCompare(String(b));
  });
}

function isRootTask(t) {
  return !t.parent || t.depth === 1;
}

function isChildTask(t) {
  return !!t.parent && t.depth > 1;
}

function isRunnableTask(t) {
  return isRootTask(t);
}

function childrenByParent(tasks) {
  const map = new Map();
  for (const task of tasks) {
    if (!task.parent) continue;
    if (!map.has(task.parent)) map.set(task.parent, []);
    map.get(task.parent).push(task);
  }
  return map;
}

function descendantsByRoot(tasks) {
  const taskById = new Map(tasks.map(t => [t.id, t]));
  const rootIds = new Set(tasks.filter(isRootTask).map(t => t.id));
  const out = new Map();
  for (const rootId of rootIds) out.set(rootId, []);
  for (const task of tasks) {
    if (!isChildTask(task)) continue;
    let cursor = task;
    let rootId = null;
    const seen = new Set();
    while (cursor && cursor.parent && !seen.has(cursor.id)) {
      seen.add(cursor.id);
      if (rootIds.has(cursor.parent)) {
        rootId = cursor.parent;
        break;
      }
      cursor = taskById.get(cursor.parent);
    }
    if (rootId && out.has(rootId)) out.get(rootId).push(task);
  }
  return out;
}

function descendantSlot(root, task) {
  const prefix = `${root.id}.`;
  return String(task.id || "").startsWith(prefix) ? String(task.id).slice(prefix.length) : String(task.id || "");
}

function cubeColumnSlots(roots, byRoot) {
  const slots = new Set();
  for (const root of roots) {
    for (const descendant of byRoot.get(root.id) || []) {
      const slot = descendantSlot(root, descendant);
      if (slot) slots.add(slot);
    }
  }
  return [...slots].sort((a, b) => String(a).localeCompare(String(b), undefined, {
    numeric: true,
    sensitivity: "base",
  }));
}

function svgEmpty(text) {
  return `<div class="empty">${escapeHtml(text)}</div>`;
}

function rowTooltip(row) {
  return escapeHtml(`${row.id} · ${row.title} · ${row.state}`);
}

function renderGantt(data) {
  const box = document.getElementById("gantt-panel");
  const tasks = data.tasks || [];
  if (!tasks.length) {
    box.innerHTML = svgEmpty("No plan loaded.");
    return;
  }
  const planState = data.plan_state || "pending";
  const rows = [
    { id: "◆", title: data.plan_title || "Plan", state: planState, level: 0, plan: true },
    ...tasks.map(t => ({
      id: t.id,
      title: t.title,
      state: t.state,
      level: isRootTask(t) ? 1 : 2,
      depth: t.depth || 1,
    })),
  ];
  const levels = [
    { level: 0, label: "LEVEL 0 - PLAN", states: uniqueSorted([planState]) },
    { level: 1, label: "LEVEL 1 - TASK", states: uniqueSorted(tasks.filter(isRootTask).map(t => t.state)) },
    { level: 2, label: "LEVEL 2 - CHILD", states: uniqueSorted(tasks.filter(isChildTask).map(t => t.state)) },
  ].filter(g => g.states.length);
  const sameAxis = levels.length > 1 && levels.every(g => g.states.join("\0") === levels[0].states.join("\0"));
  const groups = sameAxis
    ? [{ levels: levels.map(g => g.level), label: "STATE", states: levels[0].states }]
    : levels.map(g => ({ levels: [g.level], label: g.label, states: g.states }));
  const labelW = 300;
  const stateW = 104;
  const rowH = 30;
  const headerH = 54;
  const gutter = 18;
  let x = labelW;
  for (const group of groups) {
    group.x = x;
    group.w = Math.max(1, group.states.length) * stateW;
    x += group.w + gutter;
  }
  const width = Math.max(760, x + 20);
  const height = headerH + rows.length * rowH + 22;
  const groupHeaders = groups.map(group => {
    const stateLabels = group.states.map((s, i) =>
      `<text class="viz-axis" x="${group.x + i * stateW + stateW / 2}" y="42" text-anchor="middle">${escapeHtml(s)}</text>`
    ).join("");
    return `<text class="viz-axis" x="${group.x + group.w / 2}" y="18" text-anchor="middle">${escapeHtml(group.label)}</text>` +
      `<line class="viz-line" x1="${group.x}" y1="24" x2="${group.x + group.w}" y2="24"/>${stateLabels}`;
  }).join("");
  const rowSvg = rows.map((row, i) => {
    const y = headerH + i * rowH;
    const group = groups.find(g => g.levels.includes(row.level)) || groups[0];
    const col = Math.max(0, group.states.indexOf(row.state));
    const pillX = group.x + col * stateW + 12;
    const pillY = y + 7;
    const indent = row.plan ? 0 : Math.max(0, (row.depth || 1) - 1) * 14;
    const labelWeight = row.plan ? 700 : 500;
    const label = `${row.id} ${truncateLabel(row.title, row.plan ? 34 : 42)}`;
    return `<g>
      <line class="viz-line" x1="0" y1="${y}" x2="${width}" y2="${y}"/>
      <text class="viz-label" x="${12 + indent}" y="${y + 20}" font-weight="${labelWeight}">${escapeHtml(label)}</text>
      <rect x="${pillX}" y="${pillY}" width="${Math.max(58, Math.min(92, row.state.length * 7 + 18))}" height="17" rx="8" fill="${stateColor(row.state)}">
        <title>${rowTooltip(row)}</title>
      </rect>
      <text class="viz-pill-text" x="${pillX + 10}" y="${pillY + 12}">${escapeHtml(truncateLabel(row.state, 11))}</text>
    </g>`;
  }).join("");
  box.innerHTML = `<svg class="viz-svg" id="svg-gantt" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-label="Gantt visualization">
    <rect width="${width}" height="${height}" fill="transparent"/>
    <text class="viz-axis" x="12" y="36">ITEM</text>
    ${groupHeaders}
    ${rowSvg}
  </svg>`;
}

function renderCube(data) {
  const box = document.getElementById("cube-panel");
  const tasks = data.tasks || [];
  const roots = tasks.filter(isRootTask);
  if (!roots.length) {
    box.innerHTML = svgEmpty("No plan loaded.");
    return;
  }
  const byRoot = descendantsByRoot(tasks);
  const columnSlots = cubeColumnSlots(roots, byRoot);
  if (!columnSlots.length) {
    box.innerHTML = svgEmpty("This plan has no descendant tasks to render in the cube.");
    return;
  }
  const planState = data.plan_state || "pending";
  const labelW = 260;
  const taskStateW = 112;
  const cellW = 92;
  const rowH = 34;
  const headerH = 70;
  const width = Math.max(760, labelW + taskStateW + columnSlots.length * cellW + 28);
  const height = headerH + roots.length * rowH + 20;
  const headers = columnSlots.map((slot, i) =>
    `<text class="viz-axis" x="${labelW + taskStateW + i * cellW + cellW / 2}" y="60" text-anchor="middle"><title>${escapeHtml(slot)}</title>${escapeHtml(truncateLabel(slot, 10))}</text>`
  ).join("");
  const rows = roots.map((root, rowIdx) => {
    const y = headerH + rowIdx * rowH;
    const descendants = byRoot.get(root.id) || [];
    const descendantsBySlot = new Map(descendants.map(child => [descendantSlot(root, child), child]));
    const cells = columnSlots.map((slot, i) => {
      const child = descendantsBySlot.get(slot);
      const fill = child ? stateColor(child.state) : "#0f1530";
      const label = child ? truncateLabel(child.state, 9) : "";
      const title = child ? rowTooltip(child) : escapeHtml(`${root.id}.${slot} empty`);
      return `<rect x="${labelW + taskStateW + i * cellW}" y="${y + 5}" width="${cellW - 8}" height="22" rx="4" fill="${fill}" stroke="#263259">
          <title>${title}</title>
        </rect>
        <text class="viz-pill-text" x="${labelW + taskStateW + i * cellW + 8}" y="${y + 20}">${escapeHtml(label)}</text>`;
    }).join("");
    return `<g>
      <line class="viz-line" x1="0" y1="${y}" x2="${width}" y2="${y}"/>
      <text class="viz-label" x="12" y="${y + 22}">${escapeHtml(`${root.id} ${truncateLabel(root.title, 34)}`)}</text>
      <rect x="${labelW}" y="${y + 5}" width="${taskStateW - 8}" height="22" rx="4" fill="${stateColor(root.state)}"><title>${rowTooltip(root)}</title></rect>
      <text class="viz-pill-text" x="${labelW + 8}" y="${y + 20}">${escapeHtml(truncateLabel(root.state, 11))}</text>
      ${cells}
    </g>`;
  }).join("");
  box.innerHTML = `<svg class="viz-svg" id="svg-cube" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-label="Cube visualization">
    <rect width="${width}" height="${height}" fill="transparent"/>
    <rect x="0" y="0" width="${width}" height="30" rx="5" fill="${stateColor(planState)}"/>
    <text class="viz-pill-text" x="12" y="20">Plan · ${escapeHtml(planState)}</text>
    <text class="viz-axis" x="12" y="60">TASK</text>
    <text class="viz-axis" x="${labelW}" y="60">STATE</text>
    ${headers}
    ${rows}
  </svg>`;
}

function renderSankey(data) {
  const box = document.getElementById("sankey-panel");
  const tasks = data.tasks || [];
  const roots = tasks.filter(isRootTask);
  const byRoot = descendantsByRoot(tasks);
  const links = [];
  const stateTotals = new Map();
  for (const root of roots) {
    const counts = new Map();
    for (const descendant of byRoot.get(root.id) || []) {
      counts.set(descendant.state, (counts.get(descendant.state) || 0) + 1);
      stateTotals.set(descendant.state, (stateTotals.get(descendant.state) || 0) + 1);
    }
    for (const [state, count] of counts) links.push({ root, state, count });
  }
  const states = uniqueSorted([...stateTotals.keys()]);
  if (!links.length) {
    box.innerHTML = svgEmpty("This plan has no descendant-state flow to summarize.");
    return;
  }
  const planState = data.plan_state || "pending";
  const width = 920;
  const height = Math.max(260, Math.max(roots.length, states.length) * 58 + 86);
  const leftX = 240;
  const rightX = 690;
  const rootPos = new Map();
  const statePos = new Map();
  roots.forEach((root, i) => rootPos.set(root.id, 72 + i * 58));
  states.forEach((state, i) => statePos.set(state, 72 + i * 58));
  const paths = links.map(link => {
    const y1 = rootPos.get(link.root.id) + 12;
    const y2 = statePos.get(link.state) + 12;
    const stroke = Math.max(5, 4 + link.count * 4);
    return `<path d="M ${leftX + 130} ${y1} C ${leftX + 250} ${y1}, ${rightX - 120} ${y2}, ${rightX} ${y2}" fill="none" stroke="${stateColor(link.state)}" stroke-width="${stroke}" stroke-opacity="0.42">
      <title>${escapeHtml(`${link.root.id} -> ${link.state}: ${link.count}`)}</title>
    </path>`;
  }).join("");
  const rootNodes = roots.map(root => {
    const y = rootPos.get(root.id);
    const count = (byRoot.get(root.id) || []).length;
    return `<g>
      <rect x="${leftX}" y="${y}" width="130" height="24" rx="5" fill="${stateColor(root.state)}"/>
      <text class="viz-pill-text" x="${leftX + 8}" y="${y + 16}">${escapeHtml(`${root.id} (${count})`)}</text>
      <text class="viz-muted" x="12" y="${y + 16}">${escapeHtml(truncateLabel(root.title, 32))}</text>
    </g>`;
  }).join("");
  const stateNodes = states.map(state => {
    const y = statePos.get(state);
    const count = stateTotals.get(state) || 0;
    return `<g>
      <rect x="${rightX}" y="${y}" width="140" height="24" rx="5" fill="${stateColor(state)}"/>
      <text class="viz-pill-text" x="${rightX + 8}" y="${y + 16}">${escapeHtml(`${state} (${count})`)}</text>
    </g>`;
  }).join("");
  box.innerHTML = `<svg class="viz-svg" id="svg-sankey" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-label="Sankey visualization">
    <rect width="${width}" height="${height}" fill="transparent"/>
    <rect x="0" y="0" width="${width}" height="30" rx="5" fill="${stateColor(planState)}"/>
    <text class="viz-pill-text" x="12" y="20">Plan · ${escapeHtml(planState)}</text>
    <text class="viz-axis" x="${leftX}" y="54">TASKS</text>
    <text class="viz-axis" x="${rightX}" y="54">CHILD STATES</text>
    ${paths}
    ${rootNodes}
    ${stateNodes}
  </svg>`;
}

function renderTaskFilters(data) {
  const tasks = data.tasks || [];
  const runnable = tasks.filter(isRunnableTask);
  const runnableIds = new Set(runnable.map(t => t.id));
  const buckets = {
    all: tasks.length,
    running: runnable.filter(t => t.in_slot != null).length,
    ready: (data.ready || []).filter(id => runnableIds.has(id) && !(data.deferred || []).includes(id)).length,
    deferred: (data.deferred || []).filter(id => runnableIds.has(id)).length,
    blocked: runnable.filter(t => !isTerminal(t.state) && t.in_slot == null
      && !(data.ready || []).includes(t.id)
      && !(data.deferred || []).includes(t.id)).length,
    done: runnable.filter(t => isTerminal(t.state)).length,
  };
  const box = document.getElementById("task-filters");
  box.innerHTML = ["all","running","ready","deferred","blocked","done"].map(k => {
    const active = UI.taskFilter === k ? " active" : "";
    return `<button data-filter="${k}" class="${active.trim()}">${k} (${buckets[k]})</button>`;
  }).join("");
  box.querySelectorAll("button").forEach(b => b.addEventListener("click", () => {
    UI.taskFilter = b.dataset.filter;
    render(window.__lastData);
  }));
}

function blockedReasonForTask(t, taskById) {
  for (const priorId of t.prior || []) {
    const prior = taskById.get(priorId);
    if (prior && !isTerminal(prior.state)) {
      return `blocked - waiting on ${prior.id} (${prior.state})`;
    }
  }
  return null;
}

function nowKindForTask(t, data, taskById) {
  if (!isRunnableTask(t)) {
    return { cls: isTerminal(t.state) ? "muted" : "outline", text: `child state · ${t.state}` };
  }
  if (t.in_slot != null) return { cls: "busy", text: `slot ${t.in_slot} · running` };
  if ((data.deferred || []).includes(t.id)) return { cls: "warn", text: "deferred · next pass" };
  if ((data.ready || []).includes(t.id)) return { cls: "outline", text: "ready" };
  if (isTerminal(t.state)) return { cls: "muted", text: t.state };
  const reason = blockedReasonForTask(t, taskById);
  if (reason) return { cls: "warn", text: reason };
  return { cls: "muted", text: "blocked" };
}

function renderTasks(data) {
  const tbody = document.getElementById("tasks");
  const tasks = data.tasks || [];
  const hasAccounting = !!data.accounting;
  document.querySelector("#view-tasks thead tr").innerHTML = hasAccounting
    ? `<th>ID</th><th>Title</th><th>State</th><th>Assignee</th><th>Direct</th><th>Subtree</th><th>Tokens</th><th>Now</th>`
    : `<th>ID</th><th>Title</th><th>State</th><th>Assignee</th><th>Prior</th><th>Now</th>`;
  if (!tasks.length) {
    tbody.innerHTML = `<tr><td colspan="${hasAccounting ? 8 : 6}" class="empty">No plan loaded.</td></tr>`;
    return;
  }
  const taskById = new Map(tasks.map(t => [t.id, t]));
  const filtered = tasks.filter(t => {
    switch (UI.taskFilter) {
      case "running":  return isRunnableTask(t) && t.in_slot != null;
      case "ready":    return isRunnableTask(t) && (data.ready || []).includes(t.id) && !(data.deferred || []).includes(t.id);
      case "deferred": return isRunnableTask(t) && (data.deferred || []).includes(t.id);
      case "blocked":  return isRunnableTask(t) && !isTerminal(t.state) && t.in_slot == null
                                && !(data.ready || []).includes(t.id)
                                && !(data.deferred || []).includes(t.id);
      case "done":     return isRunnableTask(t) && isTerminal(t.state);
      default:         return true;
    }
  });
  if (!filtered.length) {
    tbody.innerHTML = `<tr><td colspan="${hasAccounting ? 8 : 6}" class="empty">No tasks match this filter.</td></tr>`;
    return;
  }
  tbody.innerHTML = filtered.map(t => {
    const now = nowKindForTask(t, data, taskById);
    const indent = t.depth > 1 ? "&nbsp;".repeat((t.depth - 1) * 2) + "↳ " : "";
    const stateChip = `<span class="chip" style="background:${stateColor(t.state)}">${escapeHtml(t.state)}</span>`;
    const prior = (t.prior || []).length
      ? (t.prior || []).map(p => `<span class="mono">${escapeHtml(p)}</span>`).join(", ")
      : `<span class="mono">—</span>`;
    // Result link is workspace-relative; resolve against the workspace
    // file:// URL so the browser can open it directly.
    const resultLink = t.result_link
      ? ` <a href="file://${encodeURI(data.workspace || "")}/${encodeURI(t.result_link)}" target="_blank" rel="noreferrer" title="open result file">→ result</a>`
      : "";
    const cls = t.in_slot != null ? "row-running" : (isTerminal(t.state) ? "row-done" : "");
    const direct = t.accounting && t.accounting.direct;
    const subtree = t.accounting && t.accounting.subtree;
    const accountingCells = hasAccounting
      ? `<td class="mono">${fmtCostMicro(direct && (direct.cost_micro ?? direct.priced_cost_micro), direct && direct.currency)}</td>
         <td class="mono">${fmtCostMicro(subtree && (subtree.cost_micro ?? subtree.priced_cost_micro), subtree && subtree.currency)}</td>
         <td class="mono">in ${fmtTokens(dimValue(subtree && subtree.input_total))} / out ${fmtTokens(dimValue(subtree && subtree.output_total))}</td>`
      : `<td>${prior}</td>`;
    return `<tr class="${cls}">
      <td class="mono">${indent}${escapeHtml(t.id)}</td>
      <td>${escapeHtml(t.title)}${resultLink}</td>
      <td>${stateChip}</td>
      <td class="mono">${escapeHtml(t.assignee || "—")}</td>
      ${accountingCells}
      <td><span class="chip ${now.cls}">${escapeHtml(now.text)}</span></td>
    </tr>`;
  }).join("");
}

function renderCost(data) {
  const box = document.getElementById("cost-panel");
  const accounting = data.accounting;
  if (!accounting) {
    box.innerHTML = `<div class="empty">(no accounting records found)</div>`;
    return;
  }
  const stateRows = data.accounting_by_state || [];
  const rows = [
    ["Cost", fmtCostMicro(accounting.cost_micro ?? accounting.priced_cost_micro, accounting.currency)],
    ["Input tokens", fmtTokens(dimValue(accounting.input_total))],
    ["Output tokens", fmtTokens(dimValue(accounting.output_total))],
    ["Cached input", fmtTokens(dimValue(accounting.input_cached_read))],
    ["Cached output", fmtTokens(dimValue(accounting.output_cached_read))],
    ["Coverage", accounting.coverage],
    ["Pricing", accounting.pricing_status],
    ["Invocations", accounting.invocation_count],
  ];
  const totals = `<table><tbody>${rows.map(([k, v]) =>
    `<tr><td>${escapeHtml(k)}</td><td class="mono">${escapeHtml(v)}</td></tr>`
  ).join("")}</tbody></table>`;
  const byState = stateRows.length
    ? `<table><thead><tr><th>Task</th><th>State</th><th>Invocations</th><th>In</th><th>Out</th><th>Cached In</th><th>Cost</th><th>Coverage</th></tr></thead><tbody>${stateRows.map(row => {
        const s = row.summary || {};
        return `<tr>
          <td class="mono">${escapeHtml(row.task)}</td>
          <td><span class="chip" style="background:${stateColor(row.state)}">${escapeHtml(row.state)}</span></td>
          <td class="mono">${escapeHtml(String(s.invocation_count ?? "—"))}</td>
          <td class="mono">${fmtTokens(dimValue(s.input_total))}</td>
          <td class="mono">${fmtTokens(dimValue(s.output_total))}</td>
          <td class="mono">${fmtTokens(dimValue(s.input_cached_read))}</td>
          <td class="mono">${fmtSummaryCost(s)}</td>
          <td class="mono">${escapeHtml(s.coverage || "unknown")}</td>
        </tr>`;
      }).join("")}</tbody></table>`
    : `<div class="empty">No task/state accounting rows.</div>`;
  box.innerHTML = `${totals}${byState}`;
}

function renderSlots(data) {
  const box = document.getElementById("slots-detail");
  const slots = data.slots || [];
  if (!slots.length) { box.innerHTML = `<div class="empty">No worker slots.</div>`; return; }
  // Preserve scroll positions of existing output panels keyed by slot index.
  const scrollPositions = {};
  box.querySelectorAll(".out[data-slot]").forEach(o => {
    scrollPositions[o.dataset.slot] = { top: o.scrollTop, atBottom: o.scrollHeight - o.scrollTop - o.clientHeight < 4 };
  });
  box.innerHTML = slots.map((slot, i) => renderSlotCard(slot, i, data)).join("");
  box.querySelectorAll(".out[data-slot]").forEach(o => {
    const prev = scrollPositions[o.dataset.slot];
    if (prev) {
      o.scrollTop = prev.atBottom ? o.scrollHeight : prev.top;
    } else {
      o.scrollTop = o.scrollHeight;
    }
  });
}

function renderSlotCard(slot, i, data) {
  const idle = !slot.active && !slot.task;
  if (idle) {
    return `<div class="panel slot-card idle">
      <div class="head">Worker ${i + 1} <span class="chip muted">idle</span></div>
    </div>`;
  }
  const elapsed = slot.active && slot.started_at_ms
    ? fmtDuration(data.updated_at_ms - slot.started_at_ms)
    : (slot.duration_ms != null ? fmtDuration(slot.duration_ms) : "");
  const stateChip = slot.state
    ? `<span class="chip" style="background:${stateColor(slot.state)}">${escapeHtml(slot.state)}</span>`
    : "";
  // `slot.transition` is only set for real cross-state transitions; for a
  // worker that's just running in its current state we don't show an arrow.
  const transition = slot.transition
    ? `<span class="mono">transitioned ${escapeHtml(slot.transition)}</span>`
    : (slot.state ? `<span class="mono">running in ${escapeHtml(slot.state)}</span>` : "");
  const outcome = slot.outcome
    ? `<span class="chip ${slot.outcome === "completed" ? "ok" : "warn"}">${escapeHtml(slot.outcome)}</span>`
    : "";
  const meta = [
    slot.agent ? `agent: <span class="mono">${escapeHtml(slot.agent)}</span>` : null,
    transition || null,
    elapsed ? `${slot.active ? "running" : "ran"} ${escapeHtml(elapsed)}` : null,
    slot.exit_code != null ? `exit ${slot.exit_code}` : null,
    slot.log_path ? `<a href="file://${encodeURI(slot.log_path)}" target="_blank" rel="noreferrer">view full log</a>` : null,
  ].filter(Boolean).join(" · ");
  const usage = slot.usage
    ? `<div class="meta">usage: <span class="mono">${fmtSummaryCost(slot.usage)}</span> · in <span class="mono">${fmtTokens(dimValue(slot.usage.input_total))}</span> · out <span class="mono">${fmtTokens(dimValue(slot.usage.output_total))}</span> · <span class="mono">${escapeHtml(slot.usage.coverage || "unknown")}</span></div>`
    : "";
  const traffic = (slot.traffic || []).filter(t => !UI.hideStderr || t.stream !== "stderr");
  const lines = traffic.length
    ? traffic.map(t => {
        const repeat = t.repeat > 1 ? ` <span class="repeat">×${t.repeat}</span>` : "";
        return `<div class="line"><span class="ts">${fmtClock(t.ts_ms)}</span>` +
               `<span class="stream ${t.stream}">${t.stream}</span>` +
               `<span class="text">${escapeHtml(t.text)}${repeat}</span></div>`;
      }).join("")
    : `<div class="empty">(no output yet)</div>`;
  return `<div class="panel slot-card">
    <div class="head">Worker ${i + 1} · ${escapeHtml(slot.task || "—")} ${stateChip} ${outcome}</div>
    <div class="meta">${meta}</div>
    ${usage}
    <div class="out" data-slot="${i}">${lines}</div>
  </div>`;
}

function renderJournal(data) {
  const box = document.getElementById("journal-panel");
  const list = document.getElementById("journal");
  const lines = data.recent || [];
  const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 4;
  list.innerHTML = lines.length
    ? lines.map(l => `<div class="journal-line ${l.level}">` +
        `<span class="ts">${fmtClock(l.ts_ms)}</span>` +
        `<span class="lvl">${l.level}</span>` +
        `<span class="text">${escapeHtml(l.text)}</span></div>`).join("")
    : `<div class="empty">(no events yet)</div>`;
  if (atBottom) box.scrollTop = box.scrollHeight;
}

function renderLinks(data) {
  const tbody = document.getElementById("links");
  const all = [...(data.auto_links || []), ...(data.links || [])];
  tbody.innerHTML = all.length
    ? all.map(l => `<tr>
        <td class="mono"><span class="chip muted">${escapeHtml(l.source)}</span></td>
        <td>${escapeHtml(l.label)}</td>
        <td><a href="${escapeHtml(l.url)}" target="_blank" rel="noreferrer">${escapeHtml(l.url)}</a></td>
      </tr>`).join("")
    : `<tr><td colspan="3" class="empty">No links yet.</td></tr>`;
}

function render(data) {
  window.__lastData = data;
  if (data.finished) setBanner("done", "Run finished. Snapshot is frozen at the final state.");
  renderHeader(data);
  renderLegend(data);
  setCounts(data);
  renderGantt(data);
  renderCube(data);
  renderSankey(data);
  renderTaskFilters(data);
  renderTasks(data);
  renderSlots(data);
  renderCost(data);
  renderJournal(data);
  renderLinks(data);
}

let consecutiveErrors = 0;
async function tick() {
  try {
    const res = await fetch("/snapshot", { cache: "no-store" });
    if (!res.ok) throw new Error("HTTP " + res.status);
    const data = await res.json();
    consecutiveErrors = 0;
    if (!data.finished) setBanner("", "");
    render(data);
  } catch (err) {
    consecutiveErrors++;
    // Two strikes: distinguish a transient blip from a real disconnect so a
    // fast reload doesn't flash a scary banner on every refresh.
    if (consecutiveErrors >= 2) {
      setBanner("error", "Disconnected from rhei: " + err + ". The run process may have exited.");
    }
  }
}

document.querySelectorAll(".tab").forEach(b => {
  b.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach(x => x.classList.remove("active"));
    document.querySelectorAll(".view").forEach(x => x.classList.remove("active"));
    b.classList.add("active");
    document.getElementById("view-" + b.dataset.view).classList.add("active");
  });
});

document.getElementById("hide-stderr").addEventListener("change", e => {
  UI.hideStderr = e.target.checked;
  if (window.__lastData) renderSlots(window.__lastData);
});

tick();
setInterval(tick, 1000);
</script>
</body>
</html>
"##;
