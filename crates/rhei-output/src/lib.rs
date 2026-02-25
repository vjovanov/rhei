//! Rhei Output
//!
//! Output generators for rhei plans. Currently ships JSON, GitHub-issues
//! markdown, and a terminal progress report. All generators walk the
//! recursive task tree produced by [`rhei_core::ast`].

/// Returns this crate's version.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Trait for output generators.
pub trait OutputGenerator {
    fn generate(&self, input: &str) -> String;
}

/// A no-op output generator that returns the input unchanged.
#[derive(Debug, Default, Clone)]
pub struct NoopOutput;

impl OutputGenerator for NoopOutput {
    fn generate(&self, input: &str) -> String {
        input.to_string()
    }
}

// -----------------------------------------------------------------------------
// JSON Plan Output API
// -----------------------------------------------------------------------------

use serde_json::{json, Map, Value};

use rhei_core::ast::{Rhei, Task, TaskId, TaskIdSegment};

/// Plan output generator trait for structured outputs.
pub trait PlanOutputGenerator {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value;
}

/// JSON output generator.
#[derive(Debug, Clone, Default)]
pub struct JsonOutput {
    pub pretty: bool,
}

impl PlanOutputGenerator for JsonOutput {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
        rhei_json(rhei)
    }
}

/// Convert a parsed Rhei into a serde_json::Value.
pub fn to_json_value(rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
    JsonOutput { pretty: false }.generate_rhei(rhei)
}

/// Convert a parsed Rhei into a pretty-printed JSON string.
pub fn to_json_string_pretty(rhei: &rhei_core::ast::Rhei) -> String {
    let v = to_json_value(rhei);
    serde_json::to_string_pretty(&v).expect("pretty JSON serialization")
}

// -----------------------------------------------------------------------------
// Internal helpers
// -----------------------------------------------------------------------------

fn id_segment_json(seg: &TaskIdSegment) -> Value {
    match seg {
        TaskIdSegment::Number(n) => json!({ "number": n }),
        TaskIdSegment::Named(s) => json!({ "named": s }),
    }
}

fn task_id_json(id: &TaskId) -> Value {
    let segments: Vec<Value> = id.segments.iter().map(id_segment_json).collect();
    json!({
        "path": id.to_string(),
        "segments": segments,
    })
}

fn task_json(t: &Task) -> Value {
    let prior = t.prior.iter().map(task_id_json).collect::<Vec<Value>>();
    let children = t.children.iter().map(task_json).collect::<Vec<Value>>();

    let mut obj = Map::new();
    obj.insert("id".to_string(), task_id_json(&t.id));
    obj.insert("kind".to_string(), Value::String(t.kind.clone()));
    obj.insert("title".to_string(), Value::String(t.title.clone()));
    obj.insert("state".to_string(), Value::String(t.state.clone()));
    obj.insert("prior".to_string(), Value::Array(prior));
    if let Some(ref assignee) = t.assignee {
        obj.insert("assignee".to_string(), Value::String(assignee.clone()));
    }
    if !t.content.is_empty() {
        obj.insert("content".to_string(), Value::String(t.content.clone()));
    }
    obj.insert("children".to_string(), Value::Array(children));
    Value::Object(obj)
}

fn rhei_json(rhei: &Rhei) -> Value {
    let content_sections = rhei
        .content_sections
        .iter()
        .map(|s| {
            json!({
                "title": s.title,
                "content": s.content,
            })
        })
        .collect::<Vec<Value>>();

    let tasks = rhei.tasks.iter().map(task_json).collect::<Vec<Value>>();

    json!({
        "title": rhei.title,
        "states": rhei.states,
        "structure": {
            "max_levels": rhei.structure.max_levels,
            "node_kinds": rhei.structure.node_kinds,
        },
        "frontmatter": rhei.metadata.as_ref().and_then(|metadata| serde_json::to_value(metadata).ok()),
        "content_sections": content_sections,
        "tasks": tasks
    })
}

// -----------------------------------------------------------------------------
// GitHub Issues Markdown Output API
// -----------------------------------------------------------------------------

fn title_case_kind(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len());
    let mut chars = kind.chars();
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            out.push(c);
        }
    }
    for c in chars {
        out.push(c);
    }
    out
}

fn fmt_prior_list(ids: &[TaskId]) -> String {
    ids.iter().map(|id| format!("Task {}", id)).collect::<Vec<String>>().join(", ")
}

/// GitHub issues-style Markdown output generator.
#[derive(Debug, Clone, Copy)]
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

// -----------------------------------------------------------------------------
// Progress Report (ANSI) Output API
// -----------------------------------------------------------------------------

/// Progress Report output generator for human-readable terminal summaries.
#[derive(Debug, Clone, Copy)]
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

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rhei_core::parser::parse;

    #[test]
    fn json_output_minimal_smoke() {
        let input = r#"# Rhei: Minimal

## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);

        assert_eq!(v["title"].as_str().unwrap(), "Minimal");
        assert!(v["content_sections"].is_array());

        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["id"]["path"].as_str(), Some("1"));
        assert_eq!(tasks[0]["id"]["segments"][0]["number"].as_u64(), Some(1));
        assert_eq!(tasks[0]["kind"].as_str(), Some("task"));
        assert_eq!(tasks[0]["state"].as_str(), Some("pending"));
        assert!(tasks[0]["children"].as_array().unwrap().is_empty());
    }

    #[test]
    fn json_output_with_named_ids_and_children_and_prior() {
        let input = r#"# Rhei: Complex

## Tasks

### Task 1: First
**State:** pending

#### Task 1.1: Do A
**State:** pending
Do A line

#### Task 1.2: Do B
**State:** completed
Do B line

### Task build: Build it
**State:** pending
**Prior:** Task 1
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);

        let tasks = v["tasks"].as_array().unwrap();

        let build = tasks
            .iter()
            .find(|t| t["id"]["path"].as_str() == Some("build"))
            .expect("build task exists");

        let deps = build["prior"].as_array().unwrap();
        assert!(!deps.is_empty());
        assert_eq!(deps[0]["path"].as_str(), Some("1"));

        let task1 =
            tasks.iter().find(|t| t["id"]["path"].as_str() == Some("1")).expect("task 1 exists");

        let children = task1["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["id"]["path"].as_str(), Some("1.1"));
        assert_eq!(children[0]["title"].as_str().unwrap(), "Do A");
        assert_eq!(children[1]["id"]["path"].as_str(), Some("1.2"));
        assert_eq!(children[1]["title"].as_str().unwrap(), "Do B");
    }

    #[test]
    fn json_output_omits_assignee_when_absent() {
        let input = r#"# Rhei: NoAssignee

## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);
        let tasks = v["tasks"].as_array().unwrap();
        assert!(tasks[0].as_object().unwrap().get("assignee").is_none());
    }

    #[test]
    fn json_output_includes_assignee_when_present() {
        let input = r#"# Rhei: Assigned

## Tasks

### Task 1: Alpha
**State:** in-progress
**Assignee:** alice
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);
        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["assignee"].as_str(), Some("alice"));
    }

    #[test]
    fn github_markdown_renders_assignee_line() {
        let input = r#"# Rhei: Assigned

## Tasks

### Task 1: Alpha
**State:** in-progress
**Assignee:** alice
"#;
        let rhei = parse(input).expect("parse ok");
        let s = to_github_markdown(&rhei);
        assert!(s.contains("- Assignee: alice"));
    }

    #[test]
    fn json_output_escaped_state_space() {
        let input = r#"# Rhei: Escape

## Tasks

### Task 1: One
**State:** `in progress`
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);
        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["state"].as_str(), Some("in progress"));
    }

    #[test]
    fn missing_state_is_parse_error() {
        let input = r#"# Rhei: NoState

## Tasks

### Task 1: One
"#;

        let err = parse(input).unwrap_err();
        assert!(err.message.contains("missing mandatory **State:**"));
    }

    #[test]
    fn github_markdown_tree_smoke() {
        let input = r#"# Rhei: Minimal

## Tasks

### Task 1: Alpha
**State:** pending

#### Task 1.1: Do it
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let s = to_github_markdown(&rhei);

        assert!(s.contains("# Rhei: Minimal"));
        assert!(s.contains("## Tasks"));
        assert!(s.contains("### Task 1: Alpha"));
        assert!(s.contains("#### Task 1.1: Do it"));
        assert!(s.contains("- State: pending"));
    }

    #[test]
    fn progress_report_basic_colors_and_structure() {
        let input = r#"# Rhei: Progress
## Tasks

### Task 1: Alpha
**State:** pending

#### Task 1.1: Do it
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        let gen = ProgressReportOutput { color: true, show_dependencies: true };
        let s = gen.to_string(&rhei);

        assert!(s.contains("Rhei: "));
        assert!(s.contains("* Task 1: Alpha"));
        assert!(s.contains("[PENDING]"));
        assert!(s.contains("\x1b["));
    }

    #[test]
    fn progress_report_includes_dependencies() {
        let input = r#"# Rhei: Prior
## Tasks

### Task 1: One
**State:** completed

### Task 2: Two
**State:** in progress
**Prior:** Task 1
"#;

        let rhei = parse(input).expect("parse ok");
        let s = to_progress_report(&rhei);

        assert!(s.contains("* Task 2: Two"));
        let prior_line = "  - Prior: Task 1";
        assert!(s.contains(prior_line));
    }

    #[test]
    fn progress_report_no_color_option() {
        let input = r#"# Rhei: NoColor
## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        let gen = ProgressReportOutput { color: false, show_dependencies: true };
        let s = gen.to_string(&rhei);

        assert!(!s.contains("\x1b["));
        assert!(s.contains("[PENDING]"));
    }
}
