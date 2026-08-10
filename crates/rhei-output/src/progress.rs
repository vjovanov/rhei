use std::collections::BTreeSet;

use rhei_core::ast::Task;

use crate::common::{fmt_prior_list, rhei_groups, title_case_kind, RheiGroup};

pub struct ProgressReportOutput {
    pub color: bool,
    pub show_dependencies: bool,
    /// States the resolved state machine marks final. Empty when no machine was
    /// resolved, which suppresses the completion summary rather than guessing
    /// at state names a custom machine may not use.
    pub terminal_states: BTreeSet<String>,
    /// Whether the rendered root is a Panta project rather than a single rhei.
    /// The heading names what the reader is looking at, and calling a project
    /// "Rhei:" put the same word on two different levels. §FS-rhei-panta.1
    pub is_project: bool,
}

impl ProgressReportOutput {
    /// A progress report with no state machine resolved, so no summary line.
    pub fn plain(color: bool, show_dependencies: bool) -> Self {
        Self { color, show_dependencies, terminal_states: BTreeSet::new(), is_project: false }
    }

    pub fn to_string(&self, rhei: &rhei_core::ast::Rhei) -> String {
        let mut out = String::new();

        out.push_str(if self.is_project { "Panta: " } else { "Rhei: " });
        out.push_str(&rhei.title);
        out.push('\n');
        if let Some(summary) = self.summary_line(rhei) {
            out.push_str(&summary);
            out.push('\n');
        }

        for section in &rhei.content_sections {
            if section.rhei.is_some() {
                continue;
            }
            self.render_section(&section.title, &section.content, 0, &mut out);
        }

        let groups = rhei_groups(rhei);
        if groups.is_empty() {
            for task in &rhei.tasks {
                self.render_node(task, 0, &mut out);
            }
            return out;
        }

        for group in &groups {
            out.push('\n');
            out.push_str(&group.heading());
            out.push('\n');
            for section in &rhei.content_sections {
                if section.rhei.as_deref() != Some(group.id.as_str()) || section.content.is_empty()
                {
                    continue;
                }
                self.render_section(
                    group.section_title(&section.title),
                    &section.content,
                    1,
                    &mut out,
                );
            }
            self.render_group_tasks(rhei, group, &mut out);
        }

        out
    }

    fn render_group_tasks(&self, rhei: &rhei_core::ast::Rhei, group: &RheiGroup, out: &mut String) {
        let mut empty = true;
        for task in rhei.tasks.iter().filter(|task| group.owns(task)) {
            self.render_node(task, 0, out);
            empty = false;
        }
        if empty {
            out.push_str("  (no tickets yet)\n");
        }
    }

    fn render_section(&self, title: &str, content: &str, indent: usize, out: &mut String) {
        let pad = "  ".repeat(indent);
        out.push_str(&pad);
        out.push_str(title);
        out.push_str(":\n");
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                out.push_str(&pad);
                out.push_str("  ");
                out.push_str(trimmed);
                out.push('\n');
            }
        }
    }

    /// `4/9 tickets done (44%)` — the count the format is named for.
    ///
    /// Requires resolved terminal states: "done" is a property of the state
    /// machine, and a custom machine need not have a state called `completed`.
    fn summary_line(&self, rhei: &rhei_core::ast::Rhei) -> Option<String> {
        if self.terminal_states.is_empty() {
            return None;
        }
        let (total, done) = count_tickets(&rhei.tasks, &self.terminal_states);
        if total == 0 {
            return None;
        }
        let percent = done * 100 / total;
        Some(format!("{done}/{total} tickets done ({percent}%)"))
    }

    fn render_node(&self, task: &Task, indent_level: usize, out: &mut String) {
        let state_upper = task.state.trim().to_ascii_uppercase();
        let badge = badge_for(&state_upper, self.color);

        if indent_level == 0 {
            out.push_str("* ");
        } else {
            for _ in 0..indent_level {
                out.push_str("  ");
            }
            out.push_str("- ");
        }

        out.push_str(&title_case_kind(&task.kind));
        out.push(' ');
        out.push_str(&task.id.to_string());
        out.push_str(": ");
        out.push_str(&task.title);
        out.push_str("  ");
        out.push_str(&badge);
        out.push('\n');

        if self.show_dependencies && indent_level == 0 && !task.prior.is_empty() {
            out.push_str("  - Prior: ");
            out.push_str(&fmt_prior_list(&task.prior));
            out.push('\n');
        }

        for child in &task.children {
            self.render_node(child, indent_level + 1, out);
        }
    }
}

/// `(total, in a terminal state)` over every ticket in the tree.
fn count_tickets(tasks: &[Task], terminal: &BTreeSet<String>) -> (usize, usize) {
    let mut total = 0;
    let mut done = 0;
    for task in tasks {
        total += 1;
        // A counted-loop state is authored as `<state>-<n>`; compare on the base.
        let state = task.state.trim();
        let base = state.rsplit_once('-').filter(|(_, n)| n.parse::<u32>().is_ok());
        if terminal.contains(state) || base.is_some_and(|(head, _)| terminal.contains(head)) {
            done += 1;
        }
        let (child_total, child_done) = count_tickets(&task.children, terminal);
        total += child_total;
        done += child_done;
    }
    (total, done)
}

fn badge_for(state_upper: &str, color: bool) -> String {
    if !color {
        return format!("[{}]", state_upper);
    }
    let key = state_upper.to_ascii_lowercase().replace(' ', "-");
    let code = match key.as_str() {
        "pending" => 34,     // blue
        "in-progress" => 33, // yellow
        "blocked" => 31,     // red
        "completed" => 32,   // green
        "cancelled" => 90,   // bright black / gray
        _ => 35,             // magenta (unknown)
    };
    format!("\x1b[{}m[{}]\x1b[0m", code, state_upper)
}

/// Convenience: render rhei to a colored progress report with dependencies shown.
pub fn to_progress_report(rhei: &rhei_core::ast::Rhei) -> String {
    ProgressReportOutput::plain(true, true).to_string(rhei)
}
