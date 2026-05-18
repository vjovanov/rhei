use rhei_core::ast::Task;

use crate::common::{fmt_prior_list, title_case_kind};

pub struct ProgressReportOutput {
    pub color: bool,
    pub show_dependencies: bool,
}

impl ProgressReportOutput {
    pub fn to_string(&self, rhei: &rhei_core::ast::Rhei) -> String {
        let mut out = String::new();

        out.push_str("Rhei: ");
        out.push_str(&rhei.title);
        out.push('\n');

        for section in &rhei.content_sections {
            out.push_str(&section.title);
            out.push_str(":\n");
            for line in section.content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    out.push_str("  ");
                    out.push_str(trimmed);
                    out.push('\n');
                }
            }
        }

        for task in &rhei.tasks {
            self.render_node(task, 0, &mut out);
        }

        out
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
    ProgressReportOutput { color: true, show_dependencies: true }.to_string(rhei)
}
