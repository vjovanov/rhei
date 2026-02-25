use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::event::{AgentStream, EventSink, MessageLevel, RunEvent, RunSummary, Slot, TaskOutcome};

const RECENT_LIMIT: usize = 200;
const SLOT_TRAFFIC_LIMIT: usize = 20;

pub struct DashboardSink {
    url: String,
    state: Arc<Mutex<DashboardState>>,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl DashboardSink {
    pub fn start(workspace: PathBuf, parallel: u16, total_tasks: usize) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let state =
            Arc::new(Mutex::new(DashboardState::new(workspace, parallel.max(1), total_tasks)));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || serve(listener, thread_state, thread_stop));

        Ok(Self { url, state, stop, join: Mutex::new(Some(handle)) })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn finish(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let mut guard = match self.join.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DashboardSink {
    fn drop(&mut self) {
        self.finish();
    }
}

impl EventSink for DashboardSink {
    fn emit(&self, event: RunEvent) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        state.apply(&event);
    }
}

#[derive(Clone, Serialize)]
struct DashboardState {
    workspace: String,
    parallel: u16,
    total_tasks: usize,
    pass: u32,
    ready: Vec<String>,
    slots: Vec<DashboardSlot>,
    recent: Vec<String>,
    links: Vec<DashboardLink>,
    finished: bool,
    summary: Option<DashboardSummary>,
    updated_at_ms: u128,
}

impl DashboardState {
    fn new(workspace: PathBuf, parallel: u16, total_tasks: usize) -> Self {
        Self {
            workspace: workspace.display().to_string(),
            parallel,
            total_tasks,
            pass: 0,
            ready: Vec::new(),
            slots: vec![DashboardSlot::default(); parallel as usize],
            recent: Vec::new(),
            links: Vec::new(),
            finished: false,
            summary: None,
            updated_at_ms: now_ms(),
        }
    }

    fn apply(&mut self, event: &RunEvent) {
        self.updated_at_ms = now_ms();
        match event {
            RunEvent::RunStarted { workspace, parallel, total_tasks } => {
                self.workspace = workspace.display().to_string();
                self.parallel = (*parallel).max(1);
                self.total_tasks = *total_tasks;
                self.slots = vec![DashboardSlot::default(); self.parallel as usize];
                self.push_recent(format!(
                    "run started: parallel={} total={}",
                    self.parallel, self.total_tasks
                ));
            }
            RunEvent::PassStarted { pass, ready } => {
                self.pass = *pass;
                self.ready = ready.clone();
                self.push_recent(format!("pass {pass}: {} ready", ready.len()));
            }
            RunEvent::SlotAssigned {
                slot, task, from, to, agent, log_path, wall_clock, ..
            } => {
                let slot_state = self.slot_mut(*slot);
                slot_state.active = true;
                slot_state.task = Some(task.clone());
                slot_state.agent = agent.clone();
                slot_state.state = Some(to.clone());
                slot_state.transition = Some(format!("{from}->{to}"));
                slot_state.log_path = Some(log_path.display().to_string());
                slot_state.started_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.finished_at_ms = None;
                slot_state.outcome = None;
                slot_state.traffic.clear();
                self.push_recent(format!("slot {slot}: task {task} {from}->{to}"));
            }
            RunEvent::AgentOutput { slot, stream, line, .. } => {
                let slot_state = self.slot_mut(*slot);
                if slot_state.traffic.len() == SLOT_TRAFFIC_LIMIT {
                    slot_state.traffic.remove(0);
                }
                slot_state.traffic.push(DashboardTraffic {
                    stream: match stream {
                        AgentStream::Stdout => "stdout".to_string(),
                        AgentStream::Stderr => "stderr".to_string(),
                    },
                    text: line.clone(),
                });
            }
            RunEvent::SlotReleased {
                slot,
                task,
                outcome,
                wall_clock,
                duration_ms,
                exit_code,
                ..
            } => {
                let slot_state = self.slot_mut(*slot);
                slot_state.active = false;
                slot_state.finished_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.duration_ms = Some(*duration_ms);
                slot_state.exit_code = *exit_code;
                slot_state.outcome = Some(match outcome {
                    TaskOutcome::Completed => "completed".to_string(),
                    TaskOutcome::Failed(reason) => format!("failed: {reason}"),
                    TaskOutcome::Cancelled => "cancelled".to_string(),
                    TaskOutcome::TimedOut => "timed out".to_string(),
                });
                let outcome = slot_state.outcome.as_deref().unwrap_or("unknown").to_string();
                self.push_recent(format!("slot {slot}: task {task} finished ({})", outcome));
            }
            RunEvent::PassEnded { pass, progressed } => {
                self.push_recent(format!("pass {pass} ended: progressed={progressed}"));
            }
            RunEvent::RunFinished { summary } => {
                self.finished = true;
                self.summary = Some(DashboardSummary::from(summary));
                self.push_recent(format!(
                    "run finished: terminal={}/{}",
                    summary.terminal_tasks, summary.total_tasks
                ));
            }
            RunEvent::Message { level, text } => {
                let prefix = match level {
                    MessageLevel::Info => "info",
                    MessageLevel::Warn => "warn",
                    MessageLevel::Error => "error",
                };
                self.push_recent(format!("{prefix}: {text}"));
            }
            RunEvent::RunLink { label, url } => {
                if !self.links.iter().any(|link| link.url == *url) {
                    self.links.push(DashboardLink { label: label.clone(), url: url.clone() });
                }
                self.push_recent(format!("{label}: {url}"));
            }
        }
    }

    fn slot_mut(&mut self, slot: Slot) -> &mut DashboardSlot {
        let idx = slot as usize;
        if idx >= self.slots.len() {
            self.slots.resize_with(idx + 1, DashboardSlot::default);
        }
        &mut self.slots[idx]
    }

    fn push_recent(&mut self, line: String) {
        if self.recent.len() == RECENT_LIMIT {
            self.recent.remove(0);
        }
        self.recent.push(line);
    }
}

#[derive(Clone, Default, Serialize)]
struct DashboardSlot {
    active: bool,
    task: Option<String>,
    agent: Option<String>,
    state: Option<String>,
    transition: Option<String>,
    log_path: Option<String>,
    started_at_ms: Option<u128>,
    finished_at_ms: Option<u128>,
    duration_ms: Option<u64>,
    exit_code: Option<i32>,
    outcome: Option<String>,
    traffic: Vec<DashboardTraffic>,
}

#[derive(Clone, Serialize)]
struct DashboardTraffic {
    stream: String,
    text: String,
}

#[derive(Clone, Serialize)]
struct DashboardLink {
    label: String,
    url: String,
}

#[derive(Clone, Serialize)]
struct DashboardSummary {
    agents_spawned: u32,
    programs_spawned: u32,
    terminal_tasks: usize,
    total_tasks: usize,
}

impl From<&RunSummary> for DashboardSummary {
    fn from(summary: &RunSummary) -> Self {
        Self {
            agents_spawned: summary.agents_spawned,
            programs_spawned: summary.programs_spawned,
            terminal_tasks: summary.terminal_tasks,
            total_tasks: summary.total_tasks,
        }
    }
}

fn serve(listener: TcpListener, state: Arc<Mutex<DashboardState>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => handle_client(stream, &state),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(mut stream: TcpStream, state: &Arc<Mutex<DashboardState>>) {
    let mut buf = [0u8; 2048];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let path =
        request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");

    match path {
        "/" => write_response(&mut stream, "text/html; charset=utf-8", DASHBOARD_HTML.as_bytes()),
        "/snapshot" => {
            let snapshot = match state.lock() {
                Ok(s) => s.clone(),
                Err(p) => p.into_inner().clone(),
            };
            match serde_json::to_vec(&snapshot) {
                Ok(body) => write_response(&mut stream, "application/json", &body),
                Err(err) => write_response(
                    &mut stream,
                    "application/json",
                    format!(r#"{{"error":"{}"}}"#, escape_json_string(&err.to_string())).as_bytes(),
                ),
            }
        }
        _ => write_not_found(&mut stream),
    }
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn write_not_found(stream: &mut TcpStream) {
    let body = b"not found";
    let _ = write!(
        stream,
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now())
}

fn system_time_ms(value: SystemTime) -> u128 {
    value.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

const DASHBOARD_HTML: &str = r##"<!doctype html>
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
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: var(--bg); color: var(--ink);
    font: 13px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
  header { padding: 14px 20px; background: linear-gradient(90deg, #0b1020, #131a2e);
    border-bottom: 1px solid var(--line); display: flex; gap: 16px; align-items: center; flex-wrap: wrap; }
  header h1 { margin: 0; font-size: 15px; font-weight: 600; letter-spacing: .2px; }
  header .sub { color: var(--muted); font-size: 12px; }
  .controls { display: flex; gap: 10px; align-items: center; margin-left: auto; }
  .tabs { display: flex; gap: 4px; padding: 0 20px; border-bottom: 1px solid var(--line);
    background: var(--panel); }
  button.tab { background: transparent; color: var(--muted); border: 0;
    border-bottom: 2px solid transparent; border-radius: 0; padding: 8px 14px;
    font: inherit; cursor: pointer; }
  button.tab.active { color: var(--ink); border-bottom-color: var(--accent); }
  main { padding: 18px 20px; }
  .legend { display: flex; gap: 14px; flex-wrap: wrap; color: var(--muted);
    font-size: 12px; margin-bottom: 12px; }
  .legend .sw { display: inline-block; width: 12px; height: 12px; border-radius: 3px;
    vertical-align: -2px; margin-right: 6px; }
  .view { display: none; }
  .view.active { display: block; }
  .caption { color: var(--muted); font-size: 12px; margin: 6px 2px 14px; }
  svg { background: var(--panel); border: 1px solid var(--line); border-radius: 10px;
    display: block; width: 100%; }
  text { fill: var(--ink); font: 12px -apple-system, Segoe UI, Roboto, sans-serif; }
  .row-muted { fill: var(--muted); }
  .task-label { font-weight: 600; }
  .sub-label { fill: var(--muted); font-size: 11px; }
  .pill { stroke: #0b1020; stroke-width: 1.5; cursor: default; }
  .pill:hover { filter: brightness(1.2); }
  .panel { background: var(--panel); border: 1px solid var(--line); border-radius: 10px;
    overflow: hidden; }
  .panel + .panel { margin-top: 10px; }
  table { width: 100%; border-collapse: collapse; }
  td { border-bottom: 1px solid var(--line); padding: 8px 12px; vertical-align: top;
    color: var(--ink); }
  tr:last-child td { border-bottom: 0; }
  .mono { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--muted); }
  .slot-card { padding: 10px 12px; }
  .slot-card .head { font-weight: 600; }
  .slot-card .meta { color: var(--muted); font-size: 12px; margin-top: 2px; }
  .slot-card .out { margin-top: 6px; max-height: 140px; overflow: auto; }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .tooltip { position: fixed; pointer-events: none; background: #0b1020; color: var(--ink);
    border: 1px solid var(--line); padding: 6px 8px; border-radius: 6px; font-size: 12px;
    display: none; z-index: 50; max-width: 360px; line-height: 1.4; }
  .tooltip b { color: var(--accent); }
</style>
</head>
<body>
<header>
  <h1>Rhei Run Dashboard</h1>
  <span class="sub" id="sub">live execution</span>
  <div class="controls">
    <span class="sub" id="status">Connecting…</span>
  </div>
</header>
<div class="tabs">
  <button class="tab active" data-view="slots">Agent Slots</button>
  <button class="tab" data-view="journal">Run Journal</button>
  <button class="tab" data-view="ready">Ready Tasks</button>
  <button class="tab" data-view="links">Links</button>
</div>
<main>
  <div class="legend" id="legend"></div>
  <div class="view active" id="view-slots">
    <div class="caption">One row per worker. Pill = current state of the task that worker is running. Recent output below.</div>
    <svg id="svg-slots"></svg>
    <div id="slots-detail"></div>
  </div>
  <div class="view" id="view-journal">
    <div class="caption">Most recent run-event lines, newest first.</div>
    <div class="panel"><table><tbody id="journal"></tbody></table></div>
  </div>
  <div class="view" id="view-ready">
    <div class="caption">Tasks eligible to start in the current pass.</div>
    <div class="panel"><table><tbody id="ready"></tbody></table></div>
  </div>
  <div class="view" id="view-links">
    <div class="caption">Run links emitted by the run process.</div>
    <div class="panel"><table><tbody id="links"></tbody></table></div>
  </div>
</main>
<div class="tooltip" id="tip"></div>
<script>
const STATE_ORDER = [
  "draft", "pending", "in-progress", "in_progress", "needs-review",
  "review", "prove", "consolidate", "fix", "agent-review",
  "agent-review-fix", "human-review", "completed", "blocked", "failed",
  "cancelled", "archived",
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
  "completed":        "#10b981",
  "blocked":          "#ef4444",
  "failed":           "#ef4444",
  "cancelled":        "#475569",
  "archived":         "#334155",
};
function stateColor(s) { return STATE_COLOR[s] || "#475569"; }
function stateIndex(s) {
  const i = STATE_ORDER.indexOf(s);
  return i >= 0 ? i : STATE_ORDER.length;
}

const SVG_NS = "http://www.w3.org/2000/svg";
function el(name, attrs, children) {
  const n = document.createElementNS(SVG_NS, name);
  for (const [k, v] of Object.entries(attrs || {}))
    if (v !== undefined && v !== null) n.setAttribute(k, v);
  for (const c of children || []) n.appendChild(c);
  return n;
}
function txt(s) { return document.createTextNode(s); }
function clearSvg(svg) { while (svg.firstChild) svg.removeChild(svg.firstChild); }

const tip = document.getElementById("tip");
function showTip(ev, html) {
  tip.innerHTML = html;
  tip.style.display = "block";
  tip.style.left = (ev.clientX + 12) + "px";
  tip.style.top  = (ev.clientY + 12) + "px";
}
function hideTip() { tip.style.display = "none"; }

function escapeHtml(s) {
  return String(s == null ? "" : s).replace(/[&<>"']/g, c =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
function truncate(s, n) {
  s = String(s == null ? "" : s);
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}
function fmtDuration(ms) {
  if (!ms || ms <= 0) return "";
  if (ms < 1000) return ms + "ms";
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + "s";
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return rem ? `${m}m${rem}s` : `${m}m`;
}

function collectStates(data) {
  const set = new Set();
  for (const slot of data.slots || []) if (slot.state) set.add(slot.state);
  return [...set].sort((a, b) => stateIndex(a) - stateIndex(b));
}

function renderLegend(data) {
  const box = document.getElementById("legend");
  box.innerHTML = "";
  for (const s of collectStates(data)) {
    const span = document.createElement("span");
    const sw = document.createElement("span");
    sw.className = "sw";
    sw.style.background = stateColor(s);
    span.appendChild(sw);
    span.appendChild(document.createTextNode(s));
    box.appendChild(span);
  }
}

function drawSlots(data) {
  const svg = document.getElementById("svg-slots");
  clearSvg(svg);
  const slots = data.slots || [];
  const rowH = 30;
  const leftW = 60;
  const pillW = 140;
  const topPad = 14;
  const w = 1000;
  const rows = Math.max(1, slots.length);
  const h = topPad + rows * rowH + 14;
  svg.setAttribute("viewBox", `0 0 ${w} ${h}`);
  svg.style.height = h + "px";

  if (!slots.length) {
    const t = el("text", { x: 14, y: topPad + 18, class: "row-muted" });
    t.appendChild(txt("No slots."));
    svg.appendChild(t);
    return;
  }

  slots.forEach((slot, i) => {
    const y = topPad + i * rowH;
    if (i % 2 === 0) {
      svg.appendChild(el("rect", { x: 0, y, width: w, height: rowH, fill: "#0f1530" }));
    }
    const idx = el("text", { x: 14, y: y + 19, class: "task-label" });
    idx.appendChild(txt("#" + i));
    svg.appendChild(idx);

    const state = slot.state || (slot.active ? "in_progress" : "");
    const color = state ? stateColor(state) : "#334155";
    const px = leftW;
    const py = y + 4;
    const ph = rowH - 8;
    const pill = el("rect", {
      x: px, y: py, width: pillW, height: ph, rx: 6,
      fill: color, class: "pill",
      "fill-opacity": slot.active ? 1 : 0.45,
    });
    const tipBody = `<b>Slot ${i}</b><br>` +
      `task: ${escapeHtml(slot.task || "-")}<br>` +
      `state: <b>${escapeHtml(state || "idle")}</b><br>` +
      `agent: ${escapeHtml(slot.agent || "-")}` +
      (slot.outcome ? `<br>outcome: ${escapeHtml(slot.outcome)}` : "") +
      (slot.transition ? `<br>transition: ${escapeHtml(slot.transition)}` : "");
    pill.addEventListener("mousemove", ev => showTip(ev, tipBody));
    pill.addEventListener("mouseleave", hideTip);
    svg.appendChild(pill);

    const pillLbl = el("text", {
      x: px + pillW / 2, y: py + ph / 2 + 4, "text-anchor": "middle",
      fill: "#0b1020", "font-size": 10, "font-weight": 700,
    });
    pillLbl.appendChild(txt(state || (slot.active ? "active" : "idle")));
    svg.appendChild(pillLbl);

    const parts = [];
    if (slot.task) parts.push(`task ${slot.task}`);
    if (slot.transition) parts.push(slot.transition);
    if (slot.agent) parts.push(slot.agent);
    if (slot.duration_ms) parts.push(fmtDuration(slot.duration_ms));
    const meta = parts.join("   ") || "—";
    const taskTxt = el("text", { x: leftW + pillW + 14, y: y + 19 });
    taskTxt.appendChild(txt(truncate(meta, 110)));
    svg.appendChild(taskTxt);
  });
}

function renderSlotsDetail(data) {
  const box = document.getElementById("slots-detail");
  const slots = data.slots || [];
  if (!slots.length) { box.innerHTML = ""; return; }
  box.innerHTML = slots.map((slot, i) => {
    const traffic = (slot.traffic || []).slice(-6);
    const lines = traffic.length
      ? traffic.map(t => `<div>${escapeHtml(t.stream)}&gt; ${escapeHtml(t.text)}</div>`).join("")
      : `<div>(no output)</div>`;
    return `<div class="panel slot-card">
      <div class="head">Slot ${i} · ${escapeHtml(slot.task || "—")}</div>
      <div class="meta">${escapeHtml(slot.agent || "")} ${escapeHtml(slot.transition || "")} ${escapeHtml(slot.outcome || "")}</div>
      <div class="mono out">${lines}</div>
    </div>`;
  }).join("");
}

function renderJournal(data) {
  const tbody = document.getElementById("journal");
  const lines = (data.recent || []).slice().reverse();
  tbody.innerHTML = lines.length
    ? lines.map(l => `<tr><td class="mono">${escapeHtml(l)}</td></tr>`).join("")
    : `<tr><td class="mono">(no events yet)</td></tr>`;
}

function renderReady(data) {
  const tbody = document.getElementById("ready");
  const ready = data.ready || [];
  tbody.innerHTML = ready.length
    ? ready.map(t => `<tr><td class="mono">${escapeHtml(t)}</td></tr>`).join("")
    : `<tr><td class="mono">(no ready tasks)</td></tr>`;
}

function renderLinks(data) {
  const tbody = document.getElementById("links");
  const links = data.links || [];
  tbody.innerHTML = links.length
    ? links.map(l => `<tr><td>${escapeHtml(l.label)}</td><td><a href="${escapeHtml(l.url)}" target="_blank" rel="noreferrer">${escapeHtml(l.url)}</a></td></tr>`).join("")
    : `<tr><td class="mono">(no links yet)</td></tr>`;
}

function renderStatus(data) {
  const status = document.getElementById("status");
  const sub = document.getElementById("sub");
  const ready = (data.ready || []).length;
  status.textContent = `pass ${data.pass || 0} · ready ${ready} · total ${data.total_tasks || 0} · ${data.finished ? "finished" : "running"}`;
  sub.textContent = data.workspace || "live execution";
}

function render(data) {
  renderStatus(data);
  renderLegend(data);
  drawSlots(data);
  renderSlotsDetail(data);
  renderJournal(data);
  renderReady(data);
  renderLinks(data);
}

async function tick() {
  try {
    const res = await fetch("/snapshot", { cache: "no-store" });
    if (!res.ok) throw new Error("HTTP " + res.status);
    render(await res.json());
  } catch (err) {
    document.getElementById("status").textContent = "Disconnected: " + err;
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

tick();
setInterval(tick, 1000);
</script>
</body>
</html>
"##;
