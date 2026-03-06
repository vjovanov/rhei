//! Recursive-descent style parser for Markdown plans.
//!
//! Note:
//! - This initial parser operates directly over the raw input lines to
//!   capture titles and content while respecting fenced code blocks.
//! - Integration with the Token stream will be aligned in a later step,
//!   once tokens carry sufficient payloads for titles and content.
//!
//! Responsibilities:
//! - Extract saga title and pre-Tasks content
//! - Extract tasks with metadata (State, Prior)
//! - Extract subtasks with titles and content
//! - Provide basic error reporting with line numbers
//!
//! Error recovery (Subtask 3.3) is minimally implemented: the parser
//! attempts to continue across unrecognized lines, only raising hard
//! errors for missing saga title.

use crate::ast::{ContentBlock, Saga, Subtask, Task, TaskMetadata, TaskId};
use regex::Regex;

/// Parser error with a message and an optional line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: Option<usize>,
}

impl ParseError {
    fn new<M: Into<String>>(msg: M, line: Option<usize>) -> Self {
        Self {
            message: msg.into(),
            line,
        }
    }
}

/// Result alias for parser operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Parse a markdown plan into a Saga AST.
pub fn parse(input: &str) -> Result<Saga> {
    let re_saga = Regex::new(r#"^#\s+Saga:\s+(.*)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_task_header =
        Regex::new(r#"^###\s+Task\s+([A-Za-z][A-Za-z0-9_-]*|\d+):\s+(.*)$"#).unwrap();
    let re_subtask_header = Regex::new(r#"^####\s+Subtask\s+(\d+)\.(\d+):\s+(.*)$"#).unwrap();
    let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();
    let re_prior_task_id =
        Regex::new(r#"Task\s+([A-Za-z][A-Za-z0-9_-]*|\d+)"#).unwrap();

    let mut in_code_block = false;
    let mut in_tasks_section = false;

    let mut saga_title: Option<String> = None;
    let mut saga_content: Vec<ContentBlock> = Vec::new();
    let mut tasks: Vec<Task> = Vec::new();

    // Builders
    struct TaskBuilder {
        id: TaskId,
        title: String,
        metadata: TaskMetadata,
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
        let line_no = idx + 1;

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
                } else {
                    // Task-level content is currently not modeled; ignore.
                }
            } else {
                // Saga-level content
                saga_content.push(ContentBlock::Text(raw.to_string()));
            }

            // Continue; fences do not participate in structural matching.
            continue;
        }

        let line = raw.trim();

        // Skip empty lines unless they are inside a subtask content block.
        if line.is_empty() {
            if in_tasks_section {
                if let Some(st) = cur_subtask.as_mut() {
                    st.content.push('\n');
                }
            } else if in_code_block {
                saga_content.push(ContentBlock::Text(String::new()));
            }
            continue;
        }

        if !in_tasks_section && !in_code_block {
            if let Some(cap) = re_saga.captures(line) {
                saga_title = Some(cap.get(1).unwrap().as_str().to_string());
                continue;
            }

            if re_tasks.is_match(line) {
                in_tasks_section = true;
                continue;
            }

            // Saga pre-Tasks content
            saga_content.push(ContentBlock::Text(raw.to_string()));
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
            if let Some(mut st) = cur_subtask.take() {
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
                subtasks: Vec::new(),
                metadata_closed: false,
            });

            continue;
        }

        // Subtask header
        if let Some(caps) = re_subtask_header.captures(line) {
            // Close any open subtask
            if let Some(mut st) = cur_subtask.take() {
                if let Some(t) = cur_task.as_mut() {
                    t.subtasks.push(Subtask {
                        task_number: st.task_number,
                        subtask_number: st.subtask_number,
                        title: st.title,
                        content: st.content,
                    });
                }
            }

            let task_number = caps
                .get(1)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let subtask_number = caps
                .get(2)
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let title = caps.get(3).unwrap().as_str().to_string();

            // Starting a subtask implies metadata section is closed for the task.
            if let Some(t) = cur_task.as_mut() {
                t.metadata_closed = true;
            }

            cur_subtask = Some(SubtaskBuilder {
                task_number,
                subtask_number,
                title,
                content: String::new(),
            });

            continue;
        }

        // Metadata: State
        if let Some(caps) = re_state.captures(line) {
            if let Some(t) = cur_task.as_mut() {
                if !t.metadata_closed {
                    let raw_state = caps.get(1).unwrap().as_str().trim();
                    let state = unescape_simple(raw_state);
                    t.metadata.state = Some(state);
                    continue;
                }
            }
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
            }
        }

        // Fallback: content lines
        if let Some(st) = cur_subtask.as_mut() {
            st.content.push_str(raw);
            st.content.push('\n');
        } else {
            if let Some(t) = cur_task.as_mut() {
                // Encountering non-metadata content closes the metadata window.
                t.metadata_closed = true;
            }
            // Task-level description not modeled; ignore for now.
        }
    }

    // Finalize builders
    if let Some(mut st) = cur_subtask.take() {
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
            subtasks: tb.subtasks,
        });
    }

    let title = match saga_title {
        Some(t) => t,
        None => {
            return Err(ParseError::new(
                "Missing '# Saga: <title>' header",
                None,
            ))
        }
    };

    Ok(Saga {
        title,
        content: saga_content,
        tasks,
    })
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
        let input = r#"# Saga: Example
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

        let saga = parse(input).expect("parse ok");

        assert_eq!(saga.title, "Example");
        assert!(matches!(saga.content.get(0), Some(ContentBlock::Text(s)) if s == "Some intro line"));

        assert_eq!(saga.tasks.len(), 1);
        let t1 = &saga.tasks[0];
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
    fn error_when_missing_saga_title() {
        let input = "## Tasks\n";
        let err = parse(input).unwrap_err();
        assert!(err.message.contains("Missing '# Saga"));
    }

    #[test]
    fn parses_named_task_ids_and_named_prior_dependencies() {
        let input = r#"# Saga: Example
## Tasks

### Task build_api: Build API
**State:** in-progress
**Prior:** Task setup_db, Task 2

#### Subtask 1.1: Implement endpoint
Body
"#;

        let saga = parse(input).expect("parse ok");

        assert_eq!(saga.tasks.len(), 1);
        let task = &saga.tasks[0];
        assert_eq!(task.id, TaskId::Named("build_api".to_string()));
        assert_eq!(task.title, "Build API");
        assert_eq!(task.metadata.state.as_deref(), Some("in-progress"));
        assert_eq!(
            task.metadata.depends_on,
            vec![TaskId::Named("setup_db".to_string()), TaskId::Number(2)]
        );
    }

    #[test]
    fn metadata_after_content_is_not_parsed_as_task_metadata() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: Alpha
**State:** pending
Task description closes metadata window.
**Prior:** Task 2

#### Subtask 1.1: Work
Done
"#;

        let saga = parse(input).expect("parse ok");

        assert_eq!(saga.tasks.len(), 1);
        let task = &saga.tasks[0];
        assert_eq!(task.metadata.state.as_deref(), Some("pending"));
        assert!(task.metadata.depends_on.is_empty());
        assert!(task.metadata.state_first);
        assert_eq!(task.subtasks.len(), 1);
        assert!(task.subtasks[0].content.contains("Done"));
    }

    #[test]
    fn state_after_prior_keeps_dependency_and_marks_ordering_for_validator() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: Alpha
**Prior:** Task 2
**State:** pending
"#;

        let saga = parse(input).expect("parse ok");

        assert_eq!(saga.tasks.len(), 1);
        let task = &saga.tasks[0];
        assert_eq!(task.metadata.depends_on, vec![TaskId::Number(2)]);
        assert_eq!(task.metadata.state.as_deref(), Some("pending"));
        assert!(!task.metadata.state_first);
    }

    #[test]
    fn preserves_saga_content_inside_fenced_code_blocks_before_tasks() {
        let input = r#"# Saga: Example
Intro line
```rust
### Task 999: not a real task
**State:** hidden
```
## Tasks

### Task 1: Real
**State:** pending
"#;

        let saga = parse(input).expect("parse ok");

        assert_eq!(saga.title, "Example");
        assert_eq!(
            saga.content,
            vec![
                ContentBlock::Text("Intro line".to_string()),
                ContentBlock::Text("```rust".to_string()),
                ContentBlock::Text("```".to_string()),
            ]
        );
        assert_eq!(saga.tasks.len(), 1);
        assert_eq!(saga.tasks[0].id, TaskId::Number(1));
    }
}
