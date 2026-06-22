//! The five terminal views (§FS-rhei-run-tui.1.5.3, §1.5.4) and the surroundings
//! inspector. Each renders the one shared run model under the console language;
//! view *content* is defined in §FS-rhei-viz and realized here for the terminal.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::derive::{
    has_children, inspector_chips, machine_groups, subtree_progress, task_direct, task_subtree,
    ChipAction, CostRollup,
};
use super::render::{format_cost_micro, format_tokens, render_list, state_pill};
use super::state::{FlowFocus, JournalEntry, TaskRow, UiState};
use super::text::truncate_chars;
use super::theme::{category, category_glyph};
use crate::event::{AgentStream, MessageLevel};

/// One journal line, colored by severity (color rides a prefix glyph too).
pub(super) fn journal_line(state: &UiState, entry: &JournalEntry) -> Line<'static> {
    let theme = &state.theme;
    let (prefix, color) = match entry.level {
        MessageLevel::Info => ("·", theme.dim()),
        MessageLevel::Warn => ("!", theme.category_color(super::theme::Category::Blocked)),
        MessageLevel::Error => ("✗", theme.category_color(super::theme::Category::Failed)),
    };
    Line::from(vec![
        Span::styled(format!("{prefix} "), Style::default().fg(color)),
        Span::raw(entry.text.clone()),
    ])
}

/// The Flow outline row for one task: glyph (or live marker), id, title, state
/// pill, and `done/total ✓` for parents. §FS-rhei-viz.2
fn outline_row(state: &UiState, task: &TaskRow) -> Line<'static> {
    let theme = &state.theme;
    let indent = "  ".repeat(task.depth as usize);
    let live = state.is_live(&task.id);
    let mut spans = vec![Span::raw(indent)];
    if live {
        spans.push(Span::styled(
            format!("{} ", state.spinner_glyph()),
            Style::default().fg(theme.live_color()).add_modifier(Modifier::BOLD),
        ));
    } else {
        let cat = category(&state.plan.machine, &task.state);
        spans.push(Span::styled(
            format!("{} ", category_glyph(cat)),
            Style::default().fg(theme.category_color(cat)),
        ));
    }
    spans
        .push(Span::styled(format!("{} ", task.id), Style::default().add_modifier(Modifier::BOLD)));
    spans.push(Span::raw(truncate_chars(&task.title, 32)));
    spans.push(Span::raw("  "));
    spans.extend(state_pill(theme, &state.plan.machine, &task.state));
    if let Some((done, total)) = subtree_progress(&state.plan, task) {
        spans.push(Span::styled(format!("  {done}/{total} ✓"), Style::default().fg(theme.dim())));
    }
    Line::from(spans)
}

pub(super) fn render_flow(f: &mut Frame, area: Rect, state: &UiState) {
    // Wide: panes side by side. Narrow: stack the inspector below the outline.
    let narrow = area.width < 96;
    let (outline_area, inspector_area) = if narrow {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        (chunks[0], chunks[1])
    };

    render_outline(f, outline_area, state);
    render_inspector(f, inspector_area, state);
}

fn render_outline(f: &mut Frame, area: Rect, state: &UiState) {
    let focused = matches!(state.flow_focus, FlowFocus::Outline);
    let block = Block::default()
        .title(" plan ")
        .borders(Borders::ALL)
        .border_style(border_style(state, focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let order = state.visible_task_indices();
    if order.is_empty() {
        render_placeholder(f, inner, state, "no tasks match");
        return;
    }
    let rows: Vec<(bool, Line)> = order
        .iter()
        .map(|i| {
            let task = &state.plan.tasks[*i];
            let selected = state.selected.as_deref() == Some(task.id.as_str());
            (selected, outline_row(state, task))
        })
        .collect();
    render_list(f, inner, rows, focused, &state.theme);
}

fn render_inspector(f: &mut Frame, area: Rect, state: &UiState) {
    let focused = matches!(state.flow_focus, FlowFocus::Inspector);
    let block = Block::default()
        .title(" surroundings ")
        .borders(Borders::ALL)
        .border_style(border_style(state, focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(task) = state.selected_task().cloned() else {
        render_placeholder(f, inner, state, "select a task");
        return;
    };

    let lines = inspector_lines(state, &task, focused);
    let paragraph =
        Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((state.inspector_scroll, 0));
    f.render_widget(paragraph, inner);
}

/// The surroundings inspector content, in §FS-rhei-viz.4 order. Chips are
/// numbered against `inspector_chips` so the focused chip highlights where it is
/// drawn.
fn inspector_lines(state: &UiState, task: &TaskRow, focused: bool) -> Vec<Line<'static>> {
    let theme = &state.theme;
    let machine = &state.plan.machine;
    let mut lines: Vec<Line> = Vec::new();

    // 1. Head + description.
    let mut head = vec![Span::raw("")];
    head.extend(state_pill(theme, machine, &task.state));
    head.push(Span::styled(
        format!("  {} ", task.id),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    head.push(Span::raw(task.title.clone()));
    lines.push(Line::from(head));
    let flags = task_flags(state, task);
    if !flags.is_empty() {
        lines.push(Line::from(Span::styled(flags, Style::default().fg(theme.dim()))));
    }
    if let Some(st) = state.machine_state(&task.state) {
        if let Some(desc) = &st.description {
            lines.push(Line::from(Span::styled(desc.clone(), Style::default().fg(theme.dim()))));
        }
    }
    lines.push(Line::from(""));

    // Chip numbering matches inspector_chips order: priors, unblocks, came-from, next.
    let chips = inspector_chips(state, &task.id);
    let mut chip_idx = 0usize;
    let mut emit_chip = |lines: &mut Vec<Line>, label: &str, action_is_state: bool| {
        let highlighted = focused && chip_idx == state.inspector_chip;
        let mut style = Style::default();
        if highlighted {
            style = style.fg(theme.accent()).add_modifier(Modifier::REVERSED | Modifier::BOLD);
        } else if action_is_state {
            style = style.fg(theme.dim());
        }
        lines.push(Line::from(vec![Span::raw("   "), Span::styled(label.to_string(), style)]));
        chip_idx += 1;
    };

    // 2. Depends on / unblocks.
    lines.push(section_header(theme, "depends on / unblocks"));
    let mut any_dep = false;
    for chip in &chips {
        if let ChipAction::SelectTask(_) = &chip.action {
            emit_chip(&mut lines, &chip.label, false);
            any_dep = true;
        }
    }
    if !any_dep {
        lines.push(dim_line(theme, "   (none)"));
    }
    // Waiting-on note: priors not yet terminal.
    let waiting = state.unresolved_priors(task);
    if !waiting.is_empty() {
        lines.push(dim_line(theme, &format!("   waiting on: {}", waiting.join(", "))));
    }
    lines.push(Line::from(""));

    // 3. Came from / next state.
    lines.push(section_header(theme, "came from / next"));
    let mut any_tr = false;
    for chip in &chips {
        if let ChipAction::MarkState(_) = &chip.action {
            emit_chip(&mut lines, &chip.label, true);
            any_tr = true;
        }
    }
    if !any_tr {
        lines.push(dim_line(theme, "   (terminal)"));
    }
    lines.push(Line::from(""));

    // 4. Prompt (instantiated).
    if let Some(st) = state.machine_state(&task.state) {
        if let Some(prompt) = &st.instructions {
            lines.push(section_header(theme, "prompt"));
            for raw in instantiate(prompt, task).lines().take(8) {
                lines.push(dim_line(theme, &format!("   {raw}")));
            }
            lines.push(Line::from(""));
        }
    }

    // 5. Live agent block.
    if let Some((slot, slot_state)) = state.running_slot(&task.id) {
        lines.push(section_header(theme, "live agent"));
        let elapsed = slot_state.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let mut meta = vec![Span::styled(
            format!("   slot {slot} · {elapsed}s"),
            Style::default().fg(theme.live_color()),
        )];
        if let Some(usage) = &slot_state.usage {
            let cost = usage
                .cost_micro
                .or(usage.priced_cost_micro)
                .map(format_cost_micro)
                .unwrap_or_else(|| "—".to_string());
            meta.push(Span::styled(
                format!(
                    "  {cost}  in {} out {}",
                    format_tokens(usage.input_total.value.unwrap_or(0)),
                    format_tokens(usage.output_total.value.unwrap_or(0))
                ),
                Style::default().fg(theme.dim()),
            ));
        }
        lines.push(Line::from(meta));
        for traffic in slot_state.traffic.iter().rev().take(12).rev() {
            let (label, color) = match traffic.stream {
                AgentStream::Stdout => ("out", theme.dim()),
                AgentStream::Stderr => {
                    ("err", theme.category_color(super::theme::Category::Blocked))
                }
            };
            lines.push(Line::from(vec![
                Span::styled(format!("   {label}▏ "), Style::default().fg(color)),
                Span::raw(truncate_chars(&traffic.text, 200)),
            ]));
        }
        lines.push(Line::from(""));
    }

    // 6. Artifacts.
    if let Some(st) = state.machine_state(&task.state) {
        if !st.inputs.is_empty() || !st.outputs.is_empty() {
            lines.push(section_header(theme, "artifacts"));
            for art in &st.inputs {
                let opt = if art.optional { " (optional)" } else { "" };
                lines.push(dim_line(
                    theme,
                    &format!("   in ◂ {} {}{opt}", art.name, instantiate(&art.path, task)),
                ));
            }
            for art in &st.outputs {
                let opt = if art.optional { " (optional)" } else { "" };
                lines.push(dim_line(
                    theme,
                    &format!("   out ▸ {} {}{opt}", art.name, instantiate(&art.path, task)),
                ));
            }
            lines.push(Line::from(""));
        }
    }

    // 7. Children.
    let children: Vec<&TaskRow> =
        state.plan.tasks.iter().filter(|t| t.parent.as_deref() == Some(task.id.as_str())).collect();
    if !children.is_empty() {
        let header = match subtree_progress(&state.plan, task) {
            Some((done, total)) => format!("children  {done}/{total} ✓"),
            None => "children".to_string(),
        };
        lines.push(section_header(theme, &header));
        for child in children {
            let mut spans = vec![Span::raw("   ")];
            spans.extend(state_pill(theme, machine, &child.state));
            spans.push(Span::styled(
                format!("  {} ", child.id),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(truncate_chars(&child.title, 36)));
            lines.push(Line::from(spans));
        }
    }

    lines
}

fn task_flags(state: &UiState, task: &TaskRow) -> String {
    let mut flags = Vec::new();
    if task.depth == 0 {
        flags.push("root task".to_string());
    } else {
        flags.push(format!("depth {}", task.depth));
    }
    if let Some(st) = state.machine_state(&task.state) {
        if st.initial {
            flags.push("initial".to_string());
        }
        if st.terminal {
            flags.push("terminal".to_string());
        }
        if st.gating {
            flags.push("gating".to_string());
        }
    }
    if state.deferred.contains(&task.id) {
        flags.push("deferred".to_string());
    }
    flags.join(" · ")
}

/// Resolve the scalar template variables a node can render without guessing.
fn instantiate(template: &str, task: &TaskRow) -> String {
    let mut out = template.replace("{task_id}", &task.id).replace("{task_title}", &task.title);
    if let Some(visit) = task.visit_count {
        out = out.replace("{visit_count}", &visit.to_string());
    }
    out
}

fn section_header(theme: &super::theme::Theme, label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD),
    ))
}

fn dim_line(theme: &super::theme::Theme, text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::default().fg(theme.dim())))
}

fn border_style(state: &UiState, focused: bool) -> Style {
    if focused {
        Style::default().fg(state.theme.accent())
    } else {
        Style::default().fg(state.theme.dim())
    }
}

fn render_placeholder(f: &mut Frame, area: Rect, state: &UiState, text: &str) {
    let p = Paragraph::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(state.theme.dim()),
    )));
    f.render_widget(p, area);
}

pub(super) fn render_machine(f: &mut Frame, area: Rect, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Left: grouped state list.
    let block = Block::default().title(" machine ").borders(Borders::ALL);
    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    let machine = &state.plan.machine;
    let groups = machine_groups(machine);
    let selected_state = state.selected_task().map(|task| task.state.clone());

    let visible_states = state.machine_view_order();
    let mut rows: Vec<(bool, Line)> = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        let visible_group: Vec<&usize> =
            group.iter().filter(|state_idx| visible_states.contains(state_idx)).collect();
        if visible_group.is_empty() {
            continue;
        }
        rows.push((false, dim_line(&state.theme, &format!("workflow {}", gi + 1))));
        for state_idx in visible_group {
            let st = &machine.states[*state_idx];
            let focused_row = *state_idx == state.machine_focus;
            let mut spans = vec![Span::raw(" ")];
            spans.extend(state_pill(&state.theme, machine, &st.name));
            if selected_state.as_deref() == Some(st.name.as_str()) {
                spans.push(Span::styled("  ◀ here", Style::default().fg(state.theme.live_color())));
            }
            rows.push((focused_row, Line::from(spans)));
        }
    }
    if rows.is_empty() {
        render_placeholder(f, inner, state, "no states match");
    } else {
        render_list(f, inner, rows, true, &state.theme);
    }

    // Right: state detail panel for the focused state.
    let detail_block = Block::default().title(" state ").borders(Borders::ALL);
    let detail_inner = detail_block.inner(chunks[1]);
    f.render_widget(detail_block, chunks[1]);
    if let Some(st) = machine.states.get(state.machine_focus) {
        let mut lines: Vec<Line> = Vec::new();
        let mut head = vec![Span::raw("")];
        head.extend(state_pill(&state.theme, machine, &st.name));
        lines.push(Line::from(head));
        let mut flags = Vec::new();
        if st.initial {
            flags.push("initial".to_string());
        }
        if st.terminal {
            flags.push("terminal".to_string());
        }
        if st.gating {
            flags.push("gating".to_string());
        }
        if let Some(visits) = st.visits {
            flags.push(format!("counted ×{visits}"));
        }
        if !flags.is_empty() {
            lines.push(dim_line(&state.theme, &flags.join(" · ")));
        }
        if let Some(desc) = &st.description {
            lines.push(Line::from(desc.clone()));
        }
        lines.push(Line::from(""));
        lines.push(section_header(&state.theme, "came from / next"));
        for other in &machine.states {
            if other.transitions.iter().any(|t| t.to == st.name) {
                lines.push(dim_line(&state.theme, &format!("   ⮜ {}", other.name)));
            }
        }
        for tr in &st.transitions {
            let marker = if tr.wildcard { " (from *)" } else { "" };
            lines.push(dim_line(&state.theme, &format!("   ⮞ {}{marker}", tr.to)));
        }
        // Occupying tasks.
        let occupants: Vec<&str> =
            state.plan.tasks.iter().filter(|t| t.state == st.name).map(|t| t.id.as_str()).collect();
        if !occupants.is_empty() {
            lines.push(Line::from(""));
            lines.push(section_header(&state.theme, "tasks here"));
            lines.push(dim_line(&state.theme, &format!("   {}", occupants.join(", "))));
        }
        if let Some(prompt) = &st.instructions {
            lines.push(Line::from(""));
            lines.push(section_header(&state.theme, "prompt template"));
            for raw in prompt.lines().take(10) {
                lines.push(dim_line(&state.theme, &format!("   {raw}")));
            }
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), detail_inner);
    }
}

pub(super) fn render_cost(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .title(format!(" cost — group by {} ", state.cost_group.label()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }

    let theme = &state.theme;
    let col_header = Line::from(Span::styled(
        format!(
            "{:<20} {:>10} {:>9} {:>9} {:>9} {:>9}",
            "key", "cost", "total", "in", "in cache", "out"
        ),
        Style::default().fg(theme.dim()),
    ));

    let rows = cost_rows(state);
    let mut lines: Vec<(bool, Line)> = vec![(false, col_header)];
    for (key, roll, selected) in rows {
        lines.push((selected, cost_row_line(theme, &key, &roll)));
    }
    if lines.len() == 1 {
        // Empty cost table reads like a bug unless it says why. After the run
        // finishes with nothing recorded, the agents simply reported no usage
        // (e.g. mock or non-metered agents); before then, data streams in live.
        let msg = if state.finished {
            "no cost data — agents reported no token usage for this run"
        } else {
            "no accounting yet — usage appears here as agents report it"
        };
        lines.push((false, dim_line(theme, msg)));
    }
    render_list(f, inner, lines, true, theme);
}

fn cost_row_line(theme: &super::theme::Theme, key: &str, roll: &CostRollup) -> Line<'static> {
    let cost = roll.cost_micro.map(format_cost_micro).unwrap_or_else(|| "—".to_string());
    Line::from(vec![
        Span::raw(format!("{:<20} ", truncate_chars(key, 20))),
        Span::raw(format!("{cost:>10} ")),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.total_tokens)),
            Style::default().fg(theme.dim()),
        ),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.input_tokens)),
            Style::default().fg(theme.dim()),
        ),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.input_cached_read_tokens)),
            Style::default().fg(theme.dim()),
        ),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.output_tokens)),
            Style::default().fg(theme.dim()),
        ),
    ])
}

/// Build the cost rows for the active grouping. Per-task rows carry subtree cost
/// and mark the selected task; other groupings aggregate by key.
pub(super) fn cost_rows(state: &UiState) -> Vec<(String, CostRollup, bool)> {
    use super::state::CostGroup;
    match state.cost_group {
        CostGroup::Task => state
            .visible_task_indices()
            .iter()
            .filter_map(|i| {
                let task = &state.plan.tasks[*i];
                let direct = task_direct(&state.invocations, &task.id);
                let subtree = task_subtree(&state.plan, &state.invocations, &task.id);
                if direct.invocations == 0 && subtree.invocations == 0 {
                    return None;
                }
                let roll = if has_children(&state.plan, task) { subtree } else { direct };
                let selected = state.selected.as_deref() == Some(task.id.as_str());
                Some((task.id.clone(), roll, selected))
            })
            .collect(),
        CostGroup::Agent => group_rollup(state, |u| u.agent.clone()),
        CostGroup::Model => {
            group_rollup(state, |u| u.model.clone().unwrap_or_else(|| "—".to_string()))
        }
        CostGroup::State => group_rollup(state, |u| u.state.clone()),
    }
}

fn group_rollup(
    state: &UiState,
    key_of: impl Fn(&crate::event::UsageSummary) -> String,
) -> Vec<(String, CostRollup, bool)> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, CostRollup> = BTreeMap::new();
    for rec in &state.invocations {
        groups.entry(key_of(&rec.usage)).or_default().add(&rec.usage);
    }
    groups.into_iter().enumerate().map(|(i, (k, v))| (k, v, i == state.cost_cursor)).collect()
}

pub(super) fn render_journal(f: &mut Frame, area: Rect, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(links_height(state))])
        .split(area);

    let block = Block::default()
        .title(format!(" journal — {} ", state.journal_filter.label()))
        .borders(Borders::ALL);
    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    let entries = state.filtered_journal();
    let height = inner.height as usize;
    let total = entries.len();
    // Scroll from the bottom; journal_scroll counts lines up from the tail.
    let bottom = total.saturating_sub(state.journal_scroll as usize);
    let start = bottom.saturating_sub(height);
    let lines: Vec<Line> =
        entries[start..bottom.min(total)].iter().map(|e| journal_line(state, e)).collect();
    f.render_widget(Paragraph::new(lines), inner);

    if links_height(state) > 0 {
        let links_block = Block::default().title(" links ").borders(Borders::ALL);
        let links_inner = links_block.inner(chunks[1]);
        f.render_widget(links_block, chunks[1]);
        let mut link_lines: Vec<Line> = Vec::new();
        if let Some(url) = &state.dashboard_url {
            link_lines.push(Line::from(format!("Dashboard  {url}")));
        }
        link_lines.push(Line::from(format!("Workspace  {}", state.workspace.display())));
        for link in &state.links {
            if Some(&link.url) == state.dashboard_url.as_ref() {
                continue;
            }
            link_lines.push(Line::from(format!("{}  {}", link.label, link.url)));
        }
        f.render_widget(Paragraph::new(link_lines), links_inner);
    }
}

fn links_height(state: &UiState) -> u16 {
    let mut n = 1; // workspace
    if state.dashboard_url.is_some() {
        n += 1;
    }
    n += state.links.iter().filter(|l| Some(&l.url) != state.dashboard_url.as_ref()).count();
    (n as u16 + 2).min(8)
}

pub(super) fn render_tasks(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .title(format!(
            " tasks — sort {} — state {} ",
            state.tasks_sort.label(),
            state.tasks_state_filter_label()
        ))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let theme = &state.theme;
    let col_header = Line::from(Span::styled(
        format!(
            "{:<16} {:<16} {:<8} {:<12} {:<12} {:<8}",
            "id", "state", "slot", "agent", "prior", "ready"
        ),
        Style::default().fg(theme.dim()),
    ));
    let mut rows: Vec<(bool, Line)> = vec![(false, col_header)];
    for i in state.tasks_view_order() {
        let task = &state.plan.tasks[i];
        let slot = state.running_slot(&task.id).map(|(s, _)| s.to_string()).unwrap_or_default();
        let agent =
            state.running_slot(&task.id).and_then(|(_, s)| s.agent.clone()).unwrap_or_default();
        let prior = if task.prior.is_empty() { "—".to_string() } else { task.prior.join(",") };
        let ready = state.task_ready(task);
        let selected = state.selected.as_deref() == Some(task.id.as_str());
        let mut spans = vec![Span::raw(format!("{:<16} ", truncate_chars(&task.id, 15)))];
        let cat = category(&state.plan.machine, &task.state);
        spans.push(Span::styled(
            format!("{:<16} ", truncate_chars(&task.state, 15)),
            Style::default().fg(theme.category_color(cat)),
        ));
        spans.push(Span::raw(format!("{:<8} ", truncate_chars(&slot, 7))));
        spans.push(Span::raw(format!("{:<12} ", truncate_chars(&agent, 11))));
        spans.push(Span::raw(format!("{:<12} ", truncate_chars(&prior, 11))));
        spans.push(Span::raw(format!("{ready:<8}")));
        rows.push((selected, Line::from(spans)));
    }
    render_list(f, inner, rows, true, theme);
}

/// Minimal layout: a compact one-line-per-task list with the journal strip
/// (§FS-rhei-run-tui.1.5.6).
pub(super) fn render_minimal(f: &mut Frame, area: Rect, state: &UiState) {
    let order = state.visible_task_indices();
    if order.is_empty() {
        render_placeholder(f, area, state, "no tasks");
        return;
    }
    let rows: Vec<(bool, Line)> = order
        .iter()
        .map(|i| {
            let task = &state.plan.tasks[*i];
            let selected = state.selected.as_deref() == Some(task.id.as_str());
            let mut spans = Vec::new();
            if state.is_live(&task.id) {
                spans.push(Span::styled(
                    format!("{} ", state.spinner_glyph()),
                    Style::default().fg(state.theme.live_color()),
                ));
            } else {
                let cat = category(&state.plan.machine, &task.state);
                spans.push(Span::styled(
                    format!("{} ", category_glyph(cat)),
                    Style::default().fg(state.theme.category_color(cat)),
                ));
            }
            spans.push(Span::raw(format!("{} ", task.id)));
            spans.push(Span::raw(truncate_chars(&task.title, 24)));
            (selected, Line::from(spans))
        })
        .collect();
    render_list(f, area, rows, true, &state.theme);
}
