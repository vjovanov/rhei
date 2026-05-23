use std::io::Stdout;
use std::sync::{Arc, Mutex};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use crate::event::{AgentStream, Slot};

use super::state::{UiState, UiStateSnapshot};
use super::text::truncate_chars;

pub(super) fn draw(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &Arc<Mutex<UiState>>) {
    let snapshot = match state.lock() {
        Ok(s) => s.clone_snapshot(),
        Err(p) => p.into_inner().clone_snapshot(),
    };

    let _ = terminal.draw(|f| {
        let area = f.size();
        if area.height < 4 || area.width < 20 {
            // Terminal too small — render a single line.
            let msg = Paragraph::new("rhei run (terminal too small)")
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(msg, area);
            return;
        }

        let header_height = if snapshot.dashboard_url.is_some() { 4 } else { 3 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height), // header
                Constraint::Min(snapshot.parallel.max(1) + 2),
                Constraint::Min(5), // journal pane
            ])
            .split(area);

        render_header(f, chunks[0], &snapshot);
        render_slots(f, chunks[1], &snapshot);
        render_journal(f, chunks[2], &snapshot);
    });
}

fn render_header(f: &mut ratatui::Frame, area: Rect, snapshot: &UiStateSnapshot) {
    let block = Block::default().borders(Borders::BOTTOM);
    f.render_widget(Paragraph::new(header_lines(snapshot)).block(block), area);
}

pub(super) fn header_lines(snapshot: &UiStateSnapshot) -> Vec<Line<'static>> {
    let active = snapshot.slots.iter().filter(|s| s.task.is_some()).count();
    let mut lines = vec![Line::from(vec![
        Span::styled("rhei run", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!(
            "  parallel={} active={} total_tasks={}{}",
            snapshot.parallel,
            active,
            snapshot.total_tasks,
            if snapshot.finished { "  [finished]" } else { "" },
        )),
    ])];
    if let Some(url) = &snapshot.dashboard_url {
        // §FS-rhei-run-tui.1.6: surface the live dashboard URL at the top of the TUI.
        lines.push(Line::from(vec![
            Span::styled("Dashboard: ", Style::default().fg(Color::Cyan)),
            Span::raw(url.clone()),
        ]));
    }
    lines
}

fn render_slots(f: &mut ratatui::Frame, area: Rect, snapshot: &UiStateSnapshot) {
    let block = Block::default().title(" slots ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let lines = slot_lines(snapshot, inner.width, inner.height);
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn slot_lines(
    snapshot: &UiStateSnapshot,
    width: u16,
    height: u16,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    for (idx, s) in snapshot.slots.iter().enumerate() {
        let remaining_slots = snapshot.slots.len().saturating_sub(idx + 1);
        let available_rows = height as usize;
        if lines.len() >= available_rows {
            break;
        }

        let i = idx as Slot;
        if let Some(task) = &s.task {
            let elapsed = s.started_at.map(|t| t.elapsed()).unwrap_or_default();
            let elapsed_s = elapsed.as_secs();
            let transition = s.last_event_display.as_deref().unwrap_or(&s.state);
            let task_label = s
                .agent
                .as_ref()
                .map(|agent| format!("{task} ({agent})"))
                .unwrap_or_else(|| task.clone());
            lines.push(Line::from(vec![
                Span::styled(format!("[{i:>2}] "), Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{task_label:<28}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(transition.to_string(), Style::default().fg(Color::Green)),
                Span::raw(format!("  {:>4}s", elapsed_s)),
            ]));
            let traffic_room =
                available_rows.saturating_sub(lines.len()).saturating_sub(remaining_slots);
            let traffic_tail = traffic_room.min(5);
            if traffic_tail > 0 {
                for traffic in s.traffic.iter().rev().take(traffic_tail).rev() {
                    let (label, style) = match traffic.stream {
                        AgentStream::Stdout => ("out", Style::default().fg(Color::Gray)),
                        AgentStream::Stderr => ("err", Style::default().fg(Color::Yellow)),
                    };
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(format!("{label}> "), style),
                        Span::raw(truncate_chars(&traffic.text, width.saturating_sub(11) as usize)),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("[{i:>2}] "), Style::default().fg(Color::DarkGray)),
                Span::styled("— idle —", Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    lines
}

fn render_journal(f: &mut ratatui::Frame, area: Rect, snapshot: &UiStateSnapshot) {
    let block = Block::default().title(" journal ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let height = inner.height as usize;
    let lines: Vec<Line> =
        snapshot.journal.iter().rev().take(height).rev().map(|l| Line::from(l.clone())).collect();
    f.render_widget(Paragraph::new(lines), inner);
}
