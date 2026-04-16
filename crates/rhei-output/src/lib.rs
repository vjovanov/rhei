//! Rhei Output
//!
//! Scaffold crate for output generators. Concrete implementations
//! (e.g., JSON, GitHub, progress reports) will be added later.

/// Returns this crate's version.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Trait for output generators.
///
/// Implementations will convert internal representations into
/// concrete output formats. This is a placeholder API to
/// support compilation and downstream wiring.
pub trait OutputGenerator {
    /// Generate output from a string input placeholder.
    ///
    /// This will be replaced with typed inputs once AST and validation exist.
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

use rhei_core::ast::{ContentBlock, Rhei, Subtask, Task, TaskId};

/// Plan output generator trait for structured outputs.
///
/// Implementations return a serde_json::Value tree without requiring
/// Serialize on the core AST types.
pub trait PlanOutputGenerator {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value;
}

/// JSON output generator.
///
/// pretty controls only string rendering choices in convenience methods; the
/// trait itself returns a Value.
#[derive(Debug, Clone, Default)]
pub struct JsonOutput {
    pub pretty: bool,
}

impl PlanOutputGenerator for JsonOutput {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
        rhei_json(rhei)
    }
}

// -----------------------------------------------------------------------------
// Public convenience functions
// -----------------------------------------------------------------------------

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
// Internal helpers (Value construction without Serialize derives)
// -----------------------------------------------------------------------------

fn task_id_json(id: &TaskId) -> Value {
    match id {
        TaskId::Number(n) => json!({ "number": n }),
        TaskId::Named(s) => json!({ "named": s }),
    }
}

fn subtask_json(st: &Subtask) -> Value {
    json!({
        "task_number": st.task_number,
        "subtask_number": st.subtask_number,
        "title": st.title,
        "content": st.content,
    })
}

fn task_json(t: &Task) -> Value {
    // depends_on array (possibly empty)
    let depends_on = t.metadata.depends_on.iter().map(task_id_json).collect::<Vec<Value>>();

    // metadata map with conditional "state"
    let mut meta = Map::new();
    meta.insert("depends_on".to_string(), Value::Array(depends_on));
    if let Some(state) = &t.metadata.state {
        meta.insert("state".to_string(), Value::String(state.clone()));
    }
    meta.insert("state_first".to_string(), Value::Bool(t.metadata.state_first));

    // subtasks
    let subtasks = t.subtasks.iter().map(subtask_json).collect::<Vec<Value>>();

    let mut obj = Map::new();
    obj.insert("id".to_string(), task_id_json(&t.id));
    obj.insert("title".to_string(), Value::String(t.title.clone()));
    obj.insert("metadata".to_string(), Value::Object(meta));
    if !t.content.is_empty() {
        obj.insert("content".to_string(), Value::String(t.content.clone()));
    }
    obj.insert("subtasks".to_string(), Value::Array(subtasks));

    Value::Object(obj)
}

fn rhei_json(rhei: &Rhei) -> Value {
    let content = rhei
        .content
        .iter()
        .map(|c| match c {
            ContentBlock::Text(s) => Value::String(s.clone()),
            ContentBlock::Section { title, content } => json!({
                "title": title,
                "content": content,
            }),
        })
        .collect::<Vec<Value>>();

    let tasks = rhei.tasks.iter().map(task_json).collect::<Vec<Value>>();

    json!({
        "title": rhei.title,
        "states": rhei.states,
        "content": content,
        "tasks": tasks
    })
}

// -----------------------------------------------------------------------------
// GitHub Issues Markdown Output API
// -----------------------------------------------------------------------------

/// Helper: format a TaskId as a display string without prefixes.
fn fmt_task_id(id: &TaskId) -> String {
    match id {
        TaskId::Number(n) => n.to_string(),
        TaskId::Named(s) => s.clone(),
    }
}

/// Helper: format a list of TaskIds as "Task 1, Task build".
fn fmt_prior_list(ids: &[TaskId]) -> String {
    ids.iter().map(|id| format!("Task {}", fmt_task_id(id))).collect::<Vec<String>>().join(", ")
}

/// GitHub issues-style Markdown output generator.
///
/// Controls:
/// - include_content: whether to emit subtask content indented under the checkbox item
/// - include_metadata: whether to emit "- State:" and "- Prior:" lines under each task
#[derive(Debug, Clone, Copy)]
pub struct GithubIssuesOutput {
    pub include_content: bool,
    pub include_metadata: bool,
}

impl GithubIssuesOutput {
    /// Render the provided Rhei into a single GitHub-friendly Markdown document.
    pub fn to_markdown(&self, rhei: &rhei_core::ast::Rhei) -> String {
        let mut out = String::new();

        // Title
        out.push_str("# Rhei: ");
        out.push_str(&rhei.title);
        out.push('\n');
        out.push('\n');

        // Content sections
        for block in &rhei.content {
            match block {
                ContentBlock::Text(s) => {
                    out.push_str(s);
                    out.push('\n');
                }
                ContentBlock::Section { title, content } => {
                    out.push_str("## ");
                    out.push_str(title);
                    out.push('\n');
                    if !content.is_empty() {
                        out.push_str(content);
                        out.push('\n');
                    }
                }
            }
        }
        if !rhei.content.is_empty() {
            out.push('\n');
        }

        // Tasks
        out.push_str("## Tasks\n\n");
        for task in &rhei.tasks {
            // Task header
            out.push_str("### Task ");
            out.push_str(&fmt_task_id(&task.id));
            out.push_str(": ");
            out.push_str(&task.title);
            out.push('\n');

            // Optional metadata
            if self.include_metadata {
                if let Some(state) = &task.metadata.state {
                    out.push_str("- State: ");
                    out.push_str(state);
                    out.push('\n');
                }
                if !task.metadata.depends_on.is_empty() {
                    out.push_str("- Prior: ");
                    out.push_str(&fmt_prior_list(&task.metadata.depends_on));
                    out.push('\n');
                }
            }

            // Task content
            if self.include_content && !task.content.is_empty() {
                out.push('\n');
                out.push_str(&task.content);
                out.push('\n');
            }

            // Subtasks with checkboxes
            for st in &task.subtasks {
                out.push_str("- [ ] ");
                out.push_str(&st.task_number.to_string());
                out.push('.');
                out.push_str(&st.subtask_number.to_string());
                out.push_str(": ");
                out.push_str(&st.title);
                out.push('\n');

                if self.include_content && !st.content.is_empty() {
                    for line in st.content.lines() {
                        out.push_str("  ");
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }

            out.push('\n');
        }

        out
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
    /// Whether to colorize the state badge using ANSI escape sequences.
    pub color: bool,
    /// Whether to show the "Prior" dependency list for each task.
    pub show_dependencies: bool,
}

impl ProgressReportOutput {
    /// Render the provided Rhei into a concise terminal progress report.
    ///
    /// Formatting:
    /// - Header: "Rhei: <title>"
    /// - Optional "Overview: ..." with the first non-empty rhei content line.
    /// - One-line summary per Task, with optional Prior line and subtasks.
    pub fn to_string(&self, rhei: &rhei_core::ast::Rhei) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Rhei: ");
        out.push_str(&rhei.title);
        out.push('\n');

        // Overview: first non-empty content line if any
        let first_line = rhei.content.iter().find_map(|c| match c {
            ContentBlock::Text(s) if !s.trim().is_empty() => Some(s.trim()),
            ContentBlock::Section { title, .. } => Some(title.as_str()),
            _ => None,
        });
        if let Some(line) = first_line {
            out.push_str("Overview: ");
            out.push_str(line);
            out.push('\n');
        }

        // Tasks
        for task in &rhei.tasks {
            // Determine state (uppercased for display)
            let state_upper =
                task.metadata.state.as_deref().unwrap_or("unknown").trim().to_ascii_uppercase();
            let badge = badge_for(&state_upper, self.color);

            // Task summary line
            out.push_str("* Task ");
            out.push_str(&fmt_task_id(&task.id));
            out.push_str(": ");
            out.push_str(&task.title);
            out.push_str("  ");
            out.push_str(&badge);
            out.push('\n');

            // Optional dependencies ("Prior")
            if self.show_dependencies && !task.metadata.depends_on.is_empty() {
                out.push_str("  - Prior: ");
                out.push_str(&fmt_prior_list(&task.metadata.depends_on));
                out.push('\n');
            }

            // Subtasks (no colorization, focus on task state)
            for st in &task.subtasks {
                out.push_str("  - ");
                out.push_str(&st.task_number.to_string());
                out.push('.');
                out.push_str(&st.subtask_number.to_string());
                out.push_str(": ");
                out.push_str(&st.title);
                out.push('\n');
            }
        }

        out
    }
}

fn badge_for(state_upper: &str, color: bool) -> String {
    if !color {
        return format!("[{}]", state_upper);
    }
    // Same mapping as colorize()
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

        // Rhei title
        assert_eq!(v["title"].as_str().unwrap(), "Minimal");

        // Content array exists (may be empty)
        assert!(v["content"].is_array());

        // One task with numeric id 1
        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["id"]["number"].as_u64(), Some(1));

        // Metadata.state present and set to "pending"
        assert_eq!(tasks[0]["metadata"]["state"].as_str(), Some("pending"));

        // Subtasks empty
        assert!(tasks[0]["subtasks"].as_array().unwrap().is_empty());
    }

    #[test]
    fn json_output_with_named_ids_and_subtasks_and_prior() {
        let input = r#"# Rhei: Complex

## Tasks

### Task 1: First

#### Subtask 1.1: Do A
Do A line

#### Subtask 1.2: Do B
Do B line

### Task build: Build it
**State:** pending
**Prior:** Task 1
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);

        let tasks = v["tasks"].as_array().unwrap();

        // Find named task "build"
        let build = tasks
            .iter()
            .find(|t| t["id"]["named"].as_str() == Some("build"))
            .expect("build task exists");

        // build depends_on[0].number == 1
        let deps = build["metadata"]["depends_on"].as_array().unwrap();
        assert!(!deps.is_empty());
        assert_eq!(deps[0]["number"].as_u64(), Some(1));

        // Find Task 1 and verify two subtasks with fields
        let task1 =
            tasks.iter().find(|t| t["id"]["number"].as_u64() == Some(1)).expect("task 1 exists");

        let subtasks = task1["subtasks"].as_array().unwrap();
        assert_eq!(subtasks.len(), 2);

        // Subtask 1.1
        assert_eq!(subtasks[0]["task_number"].as_u64(), Some(1));
        assert_eq!(subtasks[0]["subtask_number"].as_u64(), Some(1));
        assert_eq!(subtasks[0]["title"].as_str().unwrap(), "Do A");

        // Subtask 1.2
        assert_eq!(subtasks[1]["task_number"].as_u64(), Some(1));
        assert_eq!(subtasks[1]["subtask_number"].as_u64(), Some(2));
        assert_eq!(subtasks[1]["title"].as_str().unwrap(), "Do B");

        // Ensure state_first signal carried through for build (ordering unaffected by output)
        assert!(build["metadata"]["state_first"].is_boolean());
    }

    #[test]
    fn json_output_escaped_state_space() {
        let input = r#"# Rhei: Escape

## Tasks

### Task 1: One
**State:** in\ progress
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);
        let tasks = v["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["metadata"]["state"].as_str(), Some("in progress"));
    }

    #[test]
    fn omits_missing_state_field() {
        let input = r#"# Rhei: NoState

## Tasks

### Task 1: One
"#;

        let rhei = parse(input).expect("parse ok");
        let v = to_json_value(&rhei);
        let meta = &v["tasks"][0]["metadata"];
        // Ensure the "state" key is omitted entirely (not present)
        assert!(meta.get("state").is_none());
        // But depends_on should always be present (possibly empty)
        assert!(meta["depends_on"].is_array());
    }

    // -------------------------------------------------------------------------
    // GitHub Markdown output tests
    // -------------------------------------------------------------------------

    #[test]
    fn github_markdown_tree_smoke() {
        let input = r#"# Rhei: Minimal

## Tasks

### Task 1: Alpha
**State:** pending

#### Subtask 1.1: Do it
"#;
        let rhei = parse(input).expect("parse ok");
        let s = to_github_markdown(&rhei);

        assert!(s.contains("# Rhei: Minimal"));
        assert!(s.contains("## Tasks"));
        assert!(s.contains("### Task 1: Alpha"));
        assert!(s.contains("- State: pending"));
        assert!(s.contains("- [ ] 1.1: Do it"));
    }

    #[test]
    fn includes_prior_and_state() {
        let input = r#"# Rhei: Prior

## Tasks

### Task 2: Second
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let s = to_github_markdown(&rhei);

        assert!(s.contains("- State: pending"));
        assert!(s.contains("- Prior: Task 1"));
    }

    #[test]
    fn supports_named_ids() {
        let input = r#"# Rhei: Named

## Tasks

### Task build: Title
**State:** pending

#### Subtask 1.1: First
"#;
        let rhei = parse(input).expect("parse ok");
        let s = to_github_markdown(&rhei);

        assert!(s.contains("### Task build: Title"));
        assert!(s.contains("- [ ] 1.1: First"));
    }

    #[test]
    fn includes_content_when_enabled() {
        let input = r#"# Rhei: Content

## Tasks

### Task 1: With content
**State:** pending

#### Subtask 1.1: Do A
Line 1
Line 2
"#;
        let rhei = parse(input).expect("parse ok");
        let gen = GithubIssuesOutput { include_content: true, include_metadata: true };
        let s = gen.to_markdown(&rhei);

        // Checkbox line present
        assert!(s.contains("- [ ] 1.1: Do A"));
        // Indented content lines preserved and indented by two spaces
        assert!(s.contains("\n  Line 1\n  Line 2\n"));
    }

    #[test]
    fn omits_metadata_when_disabled() {
        let input = r#"# Rhei: NoMeta

## Tasks

### Task 1: Alpha
**State:** pending
**Prior:** Task 2

#### Subtask 1.1: Do A
"#;
        let rhei = parse(input).expect("parse ok");
        let gen = GithubIssuesOutput { include_content: false, include_metadata: false };
        let s = gen.to_markdown(&rhei);

        // Task and subtask still render
        assert!(s.contains("### Task 1: Alpha"));
        assert!(s.contains("- [ ] 1.1: Do A"));
        // Metadata omitted
        assert!(!s.contains("- State:"));
        assert!(!s.contains("- Prior:"));
    }

    // -------------------------------------------------------------------------
    // Progress Report output tests
    // -------------------------------------------------------------------------

    #[test]
    fn progress_report_basic_colors_and_structure() {
        let input = r#"# Rhei: Progress
## Tasks

### Task 1: Alpha
**State:** pending

#### Subtask 1.1: Do it
"#;

        let rhei = parse(input).expect("parse ok");
        let gen = ProgressReportOutput { color: true, show_dependencies: true };
        let s = gen.to_string(&rhei);

        assert!(s.contains("Rhei: "));
        assert!(s.contains("* Task 1: Alpha"));
        assert!(s.contains("[PENDING]"));
        // ANSI escape marker present
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

        // Second task header appears (don't match exact ANSI-wrapped badge)
        assert!(s.contains("* Task 2: Two"));
        // Prior line appears and is below the task line
        let prior_line = "  - Prior: Task 1";
        assert!(s.contains(prior_line));

        let idx_task = s.find("* Task 2: Two").expect("task 2 line index");
        let idx_prior =
            s[idx_task..].find(prior_line).map(|i| idx_task + i).expect("prior line index");
        assert!(idx_prior > idx_task);
    }

    #[test]
    fn progress_report_handles_named_ids() {
        let input = r#"# Rhei: Named
## Tasks

### Task build: Title
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        let s = to_progress_report(&rhei);

        assert!(s.contains("* Task build: Title"));
        assert!(s.contains("[PENDING]"));
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

    #[test]
    fn progress_report_escaped_state_space() {
        let input = r#"# Rhei: Escape
## Tasks

### Task 1: One
**State:** in\ progress
"#;

        let rhei = parse(input).expect("parse ok");
        let s = to_progress_report(&rhei);

        assert!(s.contains("[IN PROGRESS]"));
    }
}
