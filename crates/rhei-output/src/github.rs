use rhei_core::ast::Task;

use crate::common::{fmt_prior_list, title_case_kind};

pub struct GithubIssuesOutput {
    pub include_content: bool,
    pub include_metadata: bool,
}

impl GithubIssuesOutput {
    /// Render the provided Rhei into a single GitHub-friendly Markdown document.
    pub fn to_markdown(&self, rhei: &rhei_core::ast::Rhei) -> String {
        let mut out = String::new();

        out.push_str("# Rhei: ");
        out.push_str(&rhei.title);
        out.push('\n');
        out.push('\n');

        for section in &rhei.content_sections {
            out.push_str("## ");
            out.push_str(&section.title);
            out.push('\n');
            if !section.content.is_empty() {
                out.push_str(&section.content);
                out.push('\n');
            }
        }
        if !rhei.content_sections.is_empty() {
            out.push('\n');
        }

        out.push_str("## Tasks\n\n");
        for task in &rhei.tasks {
            self.render_node(task, 3, &mut out);
            out.push('\n');
        }

        out
    }

    fn render_node(&self, task: &Task, level: u8, out: &mut String) {
        let hashes = "#".repeat(level as usize);
        out.push_str(&hashes);
        out.push(' ');
        out.push_str(&title_case_kind(&task.kind));
        out.push(' ');
        out.push_str(&task.id.to_string());
        out.push_str(": ");
        out.push_str(&task.title);
        out.push('\n');

        if self.include_metadata {
            out.push_str("- State: ");
            out.push_str(&task.state);
            out.push('\n');
            if !task.prior.is_empty() {
                out.push_str("- Prior: ");
                out.push_str(&fmt_prior_list(&task.prior));
                out.push('\n');
            }
            if let Some(ref assignee) = task.assignee {
                out.push_str("- Assignee: ");
                out.push_str(assignee);
                out.push('\n');
            }
        }

        if self.include_content && !task.content.is_empty() {
            out.push('\n');
            out.push_str(&task.content);
            out.push('\n');
        }

        for child in &task.children {
            self.render_node(child, level + 1, out);
        }
    }
}

/// Convenience: render rhei to GitHub issues-style Markdown with all sections enabled.
pub fn to_github_markdown(rhei: &rhei_core::ast::Rhei) -> String {
    GithubIssuesOutput { include_content: true, include_metadata: true }.to_markdown(rhei)
}
