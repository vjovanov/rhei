//! Recursive-descent style parser for Markdown plans.
//!
//! Note:
//! - This initial parser operates directly over the raw input lines to
//!   capture titles and content while respecting fenced code blocks.
//! - Integration with the Token stream will be aligned in a later step,
//!   once tokens carry sufficient payloads for titles and content.
//!
//! Responsibilities:
//! - Extract rhei title and pre-Tasks content
//! - Extract tasks with metadata (State, Prior)
//! - Extract subtasks with titles and content
//! - Provide basic error reporting with line numbers
//!
//! Error recovery (Subtask 3.3) is minimally implemented: the parser
//! attempts to continue across unrecognized lines, only raising hard
//! errors for missing rhei title.

use crate::ast::{ContentBlock, Rhei, Subtask, Task, TaskId, TaskMetadata};
use regex::Regex;

/// Parser error with a message and an optional line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: Option<usize>,
}

impl ParseError {
    fn new<M: Into<String>>(msg: M, line: Option<usize>) -> Self {
        Self { message: msg.into(), line }
    }
}

/// Result alias for parser operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Parse a markdown plan into a Rhei AST.
pub fn parse(input: &str) -> Result<Rhei> {
    let re_rhei = Regex::new(r#"^#\s+Rhei:\s+(.*)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_task_header =
        Regex::new(r#"^###\s+Task\s+([A-Za-z][A-Za-z0-9_-]*|\d+):\s+(.*)$"#).unwrap();
    let re_task_like_heading = Regex::new(r#"^###\s+\S.*$"#).unwrap();
    let re_task_heading_prefix = Regex::new(r#"^###\s+Task\b.*$"#).unwrap();
    let re_subtask_header = Regex::new(r#"^####\s+Subtask\s+(\d+)\.(\d+):\s+(.*)$"#).unwrap();
    let re_subtask_like_heading = Regex::new(r#"^####\s+\S.*$"#).unwrap();
    let re_subtask_heading_prefix = Regex::new(r#"^####\s+Subtask\b.*$"#).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();
    let re_state_like = Regex::new(r#"^\*\*State\b.*$"#).unwrap();
    let re_prior_task_id = Regex::new(r#"Task\s+([A-Za-z][A-Za-z0-9_-]*|\d+)"#).unwrap();
    let re_prior_like = Regex::new(r#"^\*\*Prior\b.*$"#).unwrap();
    let re_h2_heading = Regex::new(r#"^##\s+\S.*$"#).unwrap();

    let mut in_code_block = false;
    let mut in_tasks_section = false;
    let mut tasks_section_line: Option<usize> = None;
    let mut pre_tasks_h2_seen = false;
    let mut first_nonempty_line: Option<usize> = None;

    let mut rhei_title: Option<String> = None;
    let mut rhei_header_seen = false;
    let mut rhei_states: Option<String> = None;
    let mut rhei_states_checked = false;
    let mut rhei_content: Vec<ContentBlock> = Vec::new();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();
    let mut tasks: Vec<Task> = Vec::new();

    // Builders
    struct TaskBuilder {
        id: TaskId,
        title: String,
        metadata: TaskMetadata,
        content: String,
        subtasks: Vec<Subtask>,
        // Once a non-metadata token appears after the task header,
        // we stop accepting more metadata for this task.
        metadata_closed: bool,
    }

    #[derive(Default)]
    struct SubtaskBuilder {
        task_number: u32,
        subtask_number: u32,
        title: String,
        content: String,
    }

    let mut cur_task: Option<TaskBuilder> = None;
    let mut cur_subtask: Option<SubtaskBuilder> = None;

    for (idx, raw) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw.trim();

        if !line.is_empty() {
            first_nonempty_line.get_or_insert(line_number);
        }

        // Detect fences first (outside of trimming)
        let trimmed_start = raw.trim_start();
        let is_fence = trimmed_start.starts_with("```");
        if is_fence {
            in_code_block = !in_code_block;

            // Treat fence as content in the appropriate context
            if in_tasks_section {
                if let Some(st) = cur_subtask.as_mut() {
                    st.content.push_str(raw);
                    st.content.push('\n');
                } else if let Some(t) = cur_task.as_mut() {
                    t.metadata_closed = true;
                    if !t.content.is_empty() {
                        t.content.push('\n');
                    }
                    t.content.push_str(raw);
                }
            } else {
                // Rhei-level content: append to current section if one is open
                if let Some(ContentBlock::Section { content, .. }) = rhei_content.last_mut() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(raw);
                } else {
                    rhei_content.push(ContentBlock::Text(raw.to_string()));
                }
            }

            // Continue; fences do not participate in structural matching.
            continue;
        }

        // Skip empty lines unless they are inside a subtask content block.
        if line.is_empty() {
            if in_tasks_section {
                if let Some(st) = cur_subtask.as_mut() {
                    st.content.push('\n');
                }
            } else if in_code_block {
                rhei_content.push(ContentBlock::Text(String::new()));
            }
            continue;
        }

        if !in_tasks_section && !in_code_block {
            if let Some(cap) = re_rhei.captures(line) {
                rhei_title = Some(cap.get(1).unwrap().as_str().to_string());
                rhei_header_seen = true;
                continue;
            }

            // Check for **States:** declaration (must be first non-empty line after rhei header)
            if rhei_header_seen && !rhei_states_checked {
                if let Some(cap) = re_states_decl.captures(line) {
                    rhei_states = Some(cap.get(1).unwrap().as_str().trim().to_string());
                    rhei_states_checked = true;
                    continue;
                }
                // First non-empty line after header is not **States:** — mark as checked
                rhei_states_checked = true;
            }

            // Error if **States:** appears after the first non-empty line
            if rhei_states_checked && re_states_decl.is_match(line) {
                return Err(ParseError::new(
                    "**States:** declaration must be the first non-empty line after the Rhei header",
                    Some(line_number),
                ));
            }

            let is_top_level_h1 = line.starts_with('#') && !line.starts_with("##");
            if !rhei_header_seen && is_top_level_h1 {
                return Err(ParseError::new(
                    "Malformed rhei heading: expected '# Rhei: <title>'",
                    Some(line_number),
                ));
            }

            if re_tasks.is_match(line) {
                in_tasks_section = true;
                tasks_section_line = Some(line_number);
                continue;
            }

            if re_h2_heading.is_match(line) {
                pre_tasks_h2_seen = true;
                if let Some(cap) = re_section_header.captures(line) {
                    let section_title = cap.get(1).unwrap().as_str().trim().to_string();
                    rhei_content.push(ContentBlock::Section { title: section_title, content: String::new() });
                } else {
                    rhei_content.push(ContentBlock::Text(raw.to_string()));
                }
                continue;
            }

            if re_state_like.is_match(line) || re_prior_like.is_match(line) {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            }

            // Rhei pre-Tasks content: append to current section if one is open
            if let Some(ContentBlock::Section { content, .. }) = rhei_content.last_mut() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            } else {
                rhei_content.push(ContentBlock::Text(raw.to_string()));
            }
            continue;
        }

        // From here on, we are either in tasks section or in a code block.
        if in_code_block {
            // Inside code blocks: do not match structure, append to current subtask if any.
            if let Some(st) = cur_subtask.as_mut() {
                st.content.push_str(raw);
                st.content.push('\n');
            }
            continue;
        }

        // Task header
        if let Some(caps) = re_task_header.captures(line) {
            // Finalize current subtask if present
            if let Some(st) = cur_subtask.take() {
                if let Some(t) = cur_task.as_mut() {
                    t.subtasks.push(Subtask {
                        task_number: st.task_number,
                        subtask_number: st.subtask_number,
                        title: st.title,
                        content: st.content,
                    });
                }
            }

            // Finalize previous task
            if let Some(tb) = cur_task.take() {
                tasks.push(Task {
                    id: tb.id,
                    title: tb.title,
                    metadata: tb.metadata,
                    content: tb.content,
                    subtasks: tb.subtasks,
                });
            }

            // Start a new task
            let id_str = caps.get(1).unwrap().as_str();
            let id = id_str
                .parse::<u32>()
                .ok()
                .map(TaskId::Number)
                .unwrap_or_else(|| TaskId::Named(id_str.to_string()));
            let title = caps.get(2).unwrap().as_str().to_string();

            cur_task = Some(TaskBuilder {
                id,
                title,
                metadata: TaskMetadata::default(),
                content: String::new(),
                subtasks: Vec::new(),
                metadata_closed: false,
            });

            continue;
        }

        if re_task_like_heading.is_match(line) {
            return Err(ParseError::new(
                "Malformed task heading: expected '### Task <id>: <title>'",
                Some(line_number),
            ));
        }

        // Subtask header
        if let Some(caps) = re_subtask_header.captures(line) {
            // Close any open subtask
            if let Some(st) = cur_subtask.take() {
                if let Some(t) = cur_task.as_mut() {
                    t.subtasks.push(Subtask {
                        task_number: st.task_number,
                        subtask_number: st.subtask_number,
                        title: st.title,
                        content: st.content,
                    });
                }
            }

            let task_number = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
            let subtask_number =
                caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
            let title = caps.get(3).unwrap().as_str().to_string();

            // Starting a subtask implies metadata section is closed for the task.
            if let Some(t) = cur_task.as_mut() {
                t.metadata_closed = true;
            } else {
                return Err(ParseError::new(
                    "Malformed subtask heading: expected '#### Subtask <task>.<subtask>: <title>'",
                    Some(line_number),
                ));
            }

            cur_subtask =
                Some(SubtaskBuilder { task_number, subtask_number, title, content: String::new() });

            continue;
        }

        if re_subtask_like_heading.is_match(line)
            && (cur_task.is_some() || re_subtask_heading_prefix.is_match(line))
        {
            return Err(ParseError::new(
                "Malformed subtask heading: expected '#### Subtask <task>.<subtask>: <title>'",
                Some(line_number),
            ));
        }

        // Metadata: State
        if let Some(caps) = re_state.captures(line) {
            if let Some(t) = cur_task.as_mut() {
                if !t.metadata_closed {
                    let raw_state = caps.get(1).unwrap().as_str().trim();
                    let state = unescape_state(raw_state);
                    t.metadata.state = Some(state);
                    continue;
                }

                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }

            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        if re_state_like.is_match(line) {
            if cur_task.is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**State:** <value>'",
                    Some(line_number),
                ));
            }

            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // Metadata: Prior
        if line.starts_with("**Prior:**") {
            if let Some(t) = cur_task.as_mut() {
                if !t.metadata_closed {
                    // If this is the first metadata encountered, mark state_first = false.
                    if t.metadata.state.is_none() && t.metadata.depends_on.is_empty() {
                        t.metadata.state_first = false;
                    }
                    let ids = re_prior_task_id
                        .captures_iter(line)
                        .filter_map(|c| c.get(1))
                        .map(|m| {
                            let s = m.as_str();
                            s.parse::<u32>()
                                .ok()
                                .map(TaskId::Number)
                                .unwrap_or_else(|| TaskId::Named(s.to_string()))
                        })
                        .collect::<Vec<TaskId>>();
                    t.metadata.depends_on.extend(ids);
                    continue;
                }

                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }

            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        if re_prior_like.is_match(line) {
            if cur_task.is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Prior:** Task <id>[, Task <id>...]'",
                    Some(line_number),
                ));
            }

            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        if re_task_heading_prefix.is_match(line) {
            return Err(ParseError::new(
                "Malformed task heading: expected '### Task <id>: <title>'",
                Some(line_number),
            ));
        }

        if re_subtask_heading_prefix.is_match(line) {
            return Err(ParseError::new(
                "Malformed subtask heading: expected '#### Subtask <task>.<subtask>: <title>'",
                Some(line_number),
            ));
        }

        if re_h2_heading.is_match(line) {
            return Err(ParseError::new(
                "Tasks section must be the final '##' chapter and appear as '## Tasks'",
                Some(line_number),
            ));
        }

        // Fallback: content lines
        if let Some(st) = cur_subtask.as_mut() {
            st.content.push_str(raw);
            st.content.push('\n');
        } else if let Some(t) = cur_task.as_mut() {
            // Encountering non-metadata content closes the metadata window.
            t.metadata_closed = true;
            if !t.content.is_empty() {
                t.content.push('\n');
            }
            t.content.push_str(raw);
        }
    }

    // Finalize builders
    if let Some(st) = cur_subtask.take() {
        if let Some(t) = cur_task.as_mut() {
            t.subtasks.push(Subtask {
                task_number: st.task_number,
                subtask_number: st.subtask_number,
                title: st.title,
                content: st.content,
            });
        }
    }

    if let Some(tb) = cur_task.take() {
        tasks.push(Task {
            id: tb.id,
            title: tb.title,
            metadata: tb.metadata,
            content: tb.content,
            subtasks: tb.subtasks,
        });
    }

    let title = match rhei_title {
        Some(t) => t,
        None => {
            return Err(ParseError::new(
                "Missing '# Rhei: <title>' header",
                first_nonempty_line.or(tasks_section_line),
            ));
        }
    };

    if !in_tasks_section {
        return Err(ParseError::new(
            if pre_tasks_h2_seen {
                "Tasks section must be the final '##' chapter and appear as '## Tasks'"
            } else {
                "Missing '## Tasks' section"
            },
            None,
        ));
    }

    if tasks.is_empty() {
        return Err(ParseError::new(
            "Tasks section must contain at least one task",
            tasks_section_line,
        ));
    }

    Ok(Rhei { title, states: rhei_states.unwrap_or_else(|| "rhei".to_string()), content: rhei_content, tasks })
}

/// Unescape a state value: supports backtick wrapping or backslash escaping.
fn unescape_state(input: &str) -> String {
    if input.starts_with('`') && input.ends_with('`') && input.len() >= 2 {
        return input[1..input.len() - 1].to_string();
    }
    unescape_simple(input)
}

/// Unescape simple backslash escapes used in metadata values.
///
/// For now we support:
/// - "\ " -> " "
/// - "\\" -> "\"
fn unescape_simple(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            } else {
                // Trailing backslash, keep as-is
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ContentBlock, TaskId};

    #[test]
    fn parses_minimal_plan_with_task_and_subtasks() {
        let input = r#"# Rhei: Example
Some intro line

## Tasks

### Task 1: Alpha
**State:** pending
**Prior:** Task 2

#### Subtask 1.1: Do A
Line A1
Line A2

#### Subtask 1.2: Do B
```
code block
```
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.title, "Example");
        assert!(
            matches!(rhei.content.first(), Some(ContentBlock::Text(s)) if s == "Some intro line")
        );

        assert_eq!(rhei.tasks.len(), 1);
        let t1 = &rhei.tasks[0];
        assert!(matches!(t1.id, TaskId::Number(1)));
        assert_eq!(t1.title, "Alpha");
        assert_eq!(t1.metadata.state.as_deref(), Some("pending"));
        assert_eq!(t1.metadata.depends_on, vec![TaskId::Number(2)]);

        assert_eq!(t1.subtasks.len(), 2);
        assert_eq!(t1.subtasks[0].title, "Do A");
        assert!(t1.subtasks[0].content.contains("Line A1"));
        assert!(t1.subtasks[0].content.contains("Line A2"));

        assert!(t1.subtasks[1].content.contains("```"));
        assert!(t1.subtasks[1].content.contains("code block"));
    }

    #[test]
    fn error_when_missing_rhei_title() {
        let input = "## Tasks\n";
        let err = parse(input).unwrap_err();
        assert!(err.message.contains("Missing '# Rhei"));
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn error_when_missing_rhei_title_after_leading_code_fence_points_to_first_line() {
        let input = "```md\n## Tasks\n```\n";
        let err = parse(input).unwrap_err();

        assert!(err.message.contains("Missing '# Rhei"));
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn errors_when_missing_tasks_section() {
        let input = "# Rhei: Example\n";
        let err = parse(input).unwrap_err();

        assert_eq!(err.message, "Missing '## Tasks' section");
        assert_eq!(err.line, None);
    }

    #[test]
    fn errors_when_tasks_section_is_empty() {
        let input = "# Rhei: Example\n## Tasks\n";
        let err = parse(input).unwrap_err();

        assert_eq!(err.message, "Tasks section must contain at least one task");
        assert_eq!(err.line, Some(2));
    }

    #[test]
    fn errors_on_malformed_rhei_heading_intended_as_rhei_header() {
        let input = "#Rhei: Example\n## Tasks\n";
        let err = parse(input).unwrap_err();

        assert_eq!(err.message, "Malformed rhei heading: expected '# Rhei: <title>'");
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn errors_on_h1_heading_with_wrong_keyword_as_malformed_rhei_heading() {
        let input = "# Sga: Example\n## Tasks\n";
        let err = parse(input).unwrap_err();

        assert_eq!(err.message, "Malformed rhei heading: expected '# Rhei: <title>'");
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn allows_arbitrary_h2_chapters_before_tasks_section() {
        let input = r#"# Rhei: Example

## Overview
High-level context.

## Requirements
- Preserve audit logs
- Support approvals

## Tasks

### Task 1: Alpha
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.title, "Example");
        assert_eq!(rhei.tasks.len(), 1);
        assert_eq!(
            rhei.content,
            vec![
                ContentBlock::Section {
                    title: "Overview".to_string(),
                    content: "High-level context.".to_string(),
                },
                ContentBlock::Section {
                    title: "Requirements".to_string(),
                    content: "- Preserve audit logs\n- Support approvals".to_string(),
                },
            ]
        );
    }

    #[test]
    fn errors_when_tasks_section_is_not_final_h2_chapter() {
        let input = r#"# Rhei: Example

## Overview
Context before tasks.

## Tasks

### Task 1: Alpha
**State:** pending

## Appendix
Trailing chapter after tasks.
"#;
        let err = parse(input).unwrap_err();

        assert_eq!(
            err.message,
            "Tasks section must be the final '##' chapter and appear as '## Tasks'"
        );
        assert_eq!(err.line, Some(11));
    }

    #[test]
    fn parses_named_task_ids_and_named_prior_dependencies() {
        let input = r#"# Rhei: Example
## Tasks

### Task build_api: Build API
**State:** in-progress
**Prior:** Task setup_db, Task 2

#### Subtask 1.1: Implement endpoint
Body
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.tasks.len(), 1);
        let task = &rhei.tasks[0];
        assert_eq!(task.id, TaskId::Named("build_api".to_string()));
        assert_eq!(task.title, "Build API");
        assert_eq!(task.metadata.state.as_deref(), Some("in-progress"));
        assert_eq!(
            task.metadata.depends_on,
            vec![TaskId::Named("setup_db".to_string()), TaskId::Number(2)]
        );
    }

    #[test]
    fn errors_when_metadata_after_content() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
Task description closes metadata window.
**Prior:** Task 2

#### Subtask 1.1: Work
Done
"#;

        let err = parse(input).unwrap_err();

        assert_eq!(
            err.message,
            "Metadata fields must appear immediately after the task heading before task content"
        );
        assert_eq!(err.line, Some(7));
    }

    #[test]
    fn state_after_prior_keeps_dependency_and_marks_ordering_for_validator() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Prior:** Task 2
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.tasks.len(), 1);
        let task = &rhei.tasks[0];
        assert_eq!(task.metadata.depends_on, vec![TaskId::Number(2)]);
        assert_eq!(task.metadata.state.as_deref(), Some("pending"));
        assert!(!task.metadata.state_first);
    }

    #[test]
    fn preserves_rhei_content_inside_fenced_code_blocks_before_tasks() {
        let input = r#"# Rhei: Example
Intro line
```rust
### Task 999: not a real task
**State:** hidden
```
## Tasks

### Task 1: Real
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.title, "Example");
        assert_eq!(
            rhei.content,
            vec![
                ContentBlock::Text("Intro line".to_string()),
                ContentBlock::Text("```rust".to_string()),
                ContentBlock::Text("```".to_string()),
            ]
        );
        assert_eq!(rhei.tasks.len(), 1);
        assert_eq!(rhei.tasks[0].id, TaskId::Number(1));
    }

    #[test]
    fn errors_on_malformed_task_heading_in_tasks_section() {
        let input = r#"# Rhei: Example
## Tasks

### Tak 3: Broken heading
**State:** pending

#### Subtask 3.1: Dry run
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Malformed task heading: expected '### Task <id>: <title>'");
        assert_eq!(err.line, Some(4));
    }

    #[test]
    fn errors_on_task_heading_missing_colon_separator() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1 Broken heading
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Malformed task heading: expected '### Task <id>: <title>'");
        assert_eq!(err.line, Some(4));
    }

    #[test]
    fn errors_on_empty_task_title() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1:
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Malformed task heading: expected '### Task <id>: <title>'");
        assert_eq!(err.line, Some(4));
    }

    #[test]
    fn errors_on_malformed_subtask_heading() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

#### Subtak 1.1: Broken
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(
            err.message,
            "Malformed subtask heading: expected '#### Subtask <task>.<subtask>: <title>'"
        );
        assert_eq!(err.line, Some(7));
    }

    #[test]
    fn errors_on_empty_subtask_title() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

#### Subtask 1.1:
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(
            err.message,
            "Malformed subtask heading: expected '#### Subtask <task>.<subtask>: <title>'"
        );
        assert_eq!(err.line, Some(7));
    }

    #[test]
    fn errors_on_malformed_state_metadata_line() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State** pending
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Malformed metadata field: expected '**State:** <value>'");
        assert_eq!(err.line, Some(5));
    }

    #[test]
    fn errors_on_malformed_prior_metadata_line() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Prior** Task 2
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(
            err.message,
            "Malformed metadata field: expected '**Prior:** Task <id>[, Task <id>...]'"
        );
        assert_eq!(err.line, Some(5));
    }

    #[test]
    fn errors_on_metadata_outside_task_before_tasks_section() {
        let input = r#"# Rhei: Example
**State:** pending
## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Metadata field appears outside a task");
        assert_eq!(err.line, Some(2));
    }

    #[test]
    fn errors_on_metadata_outside_task_inside_tasks_section() {
        let input = r#"# Rhei: Example
## Tasks
**State:** pending

### Task 1: Alpha
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Metadata field appears outside a task");
        assert_eq!(err.line, Some(3));
    }

    #[test]
    fn does_not_treat_non_task_third_level_heading_as_malformed_inside_rhei_content() {
        let input = r#"# Rhei: Example

### Notes
This is rhei content before tasks.

## Tasks

### Task 1: Real
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        assert_eq!(rhei.tasks.len(), 1);
        assert_eq!(rhei.tasks[0].id, TaskId::Number(1));
    }
}
