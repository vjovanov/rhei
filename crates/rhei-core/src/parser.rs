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

use crate::ast::{ContentSection, Metadata, Rhei, Subtask, Task, TaskId};
use regex::Regex;
use serde_yaml::Value as YamlValue;

/// Parser error with a message and an optional line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: Option<usize>,
}

impl ParseError {
    pub fn new<M: Into<String>>(msg: M, line: Option<usize>) -> Self {
        Self { message: msg.into(), line }
    }
}

/// Result alias for parser operations.
pub type Result<T> = std::result::Result<T, ParseError>;

fn parse_frontmatter(lines: &[String], start_line: usize, kind: &str) -> Result<Metadata> {
    if lines.is_empty() || lines.iter().all(|line| line.trim().is_empty()) {
        return Ok(Metadata::new());
    }

    let yaml = lines.join("\n");
    let value: YamlValue = serde_yaml::from_str(&yaml).map_err(|err| {
        ParseError::new(format!("failed to parse {kind} YAML frontmatter: {err}"), Some(start_line))
    })?;

    match value {
        YamlValue::Mapping(mapping) => Ok(mapping),
        _ => Err(ParseError::new(
            format!("{kind} YAML frontmatter must be a mapping"),
            Some(start_line),
        )),
    }
}

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
    let mut rhei_metadata: Option<Metadata> = None;
    let mut frontmatter_checked = false;
    let mut in_frontmatter = false;
    let mut frontmatter_start_line = 0usize;
    let mut frontmatter_lines: Vec<String> = Vec::new();
    let mut rhei_content: Vec<ContentSection> = Vec::new();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();
    let mut tasks: Vec<Task> = Vec::new();

    // Builders
    struct TaskBuilder {
        id: TaskId,
        title: String,
        state: Option<String>,
        prior: Vec<TaskId>,
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
        state: Option<String>,
        content: String,
        /// Once a non-metadata token appears after the subtask header,
        /// we stop accepting metadata for this subtask.
        metadata_closed: bool,
    }

    let mut cur_task: Option<TaskBuilder> = None;
    let mut cur_subtask: Option<SubtaskBuilder> = None;

    for (idx, raw) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw.trim();

        if !line.is_empty() {
            first_nonempty_line.get_or_insert(line_number);
        }

        if in_frontmatter {
            if line == "---" {
                rhei_metadata =
                    Some(parse_frontmatter(&frontmatter_lines, frontmatter_start_line, "plan")?);
                in_frontmatter = false;
                continue;
            }
            frontmatter_lines.push(raw.to_string());
            continue;
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
                if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(raw);
                }
                // Loose text outside a section is silently ignored per spec.
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
                // Empty line inside a code block within a section
                if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                    content.push('\n');
                }
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

            if rhei_header_seen && rhei_states_checked && !frontmatter_checked {
                if line == "---" {
                    in_frontmatter = true;
                    frontmatter_checked = true;
                    frontmatter_start_line = line_number + 1;
                    frontmatter_lines.clear();
                    continue;
                }
                frontmatter_checked = true;
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
                    rhei_content
                        .push(ContentSection { title: section_title, content: String::new() });
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
            if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            }
            // Loose text outside a section is silently ignored per spec.
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
                    let state = st.state.ok_or_else(|| {
                        ParseError::new(
                            format!(
                                "Subtask {}.{} is missing mandatory **State:** metadata",
                                st.task_number, st.subtask_number
                            ),
                            Some(line_number),
                        )
                    })?;
                    t.subtasks.push(Subtask {
                        task_number: st.task_number,
                        subtask_number: st.subtask_number,
                        title: st.title,
                        state,
                        content: st.content,
                    });
                }
            }

            // Finalize previous task
            if let Some(tb) = cur_task.take() {
                let state = tb.state.ok_or_else(|| {
                    ParseError::new(
                        format!("Task {} is missing mandatory **State:** metadata", tb.id),
                        Some(line_number),
                    )
                })?;
                tasks.push(Task {
                    id: tb.id,
                    title: tb.title,
                    state,
                    prior: tb.prior,
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
                state: None,
                prior: Vec::new(),
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
                    let state = st.state.ok_or_else(|| {
                        ParseError::new(
                            format!(
                                "Subtask {}.{} is missing mandatory **State:** metadata",
                                st.task_number, st.subtask_number
                            ),
                            Some(line_number),
                        )
                    })?;
                    t.subtasks.push(Subtask {
                        task_number: st.task_number,
                        subtask_number: st.subtask_number,
                        title: st.title,
                        state,
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

            cur_subtask = Some(SubtaskBuilder {
                task_number,
                subtask_number,
                title,
                state: None,
                content: String::new(),
                metadata_closed: false,
            });

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

        // Metadata: State (for tasks and subtasks)
        if let Some(caps) = re_state.captures(line) {
            // Check subtask context first
            if let Some(st) = cur_subtask.as_mut() {
                if !st.metadata_closed {
                    let raw_state = caps.get(1).unwrap().as_str().trim();
                    let state = unescape_state(raw_state);
                    st.state = Some(state);
                    continue;
                }

                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the subtask heading before subtask content",
                    Some(line_number),
                ));
            }

            if let Some(t) = cur_task.as_mut() {
                if !t.metadata_closed {
                    let raw_state = caps.get(1).unwrap().as_str().trim();
                    let state = unescape_state(raw_state);
                    t.state = Some(state);
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
                    // Spec: **State:** must appear before **Prior:**
                    if t.state.is_none() {
                        return Err(ParseError::new(
                            format!("**State:** must appear before **Prior:** for Task {}", t.id),
                            Some(line_number),
                        ));
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
                    t.prior.extend(ids);
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
            st.metadata_closed = true;
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

    if in_frontmatter {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(frontmatter_start_line.saturating_sub(1).max(1)),
        ));
    }

    // Finalize builders
    if let Some(st) = cur_subtask.take() {
        if let Some(t) = cur_task.as_mut() {
            let state = st.state.ok_or_else(|| {
                ParseError::new(
                    format!(
                        "Subtask {}.{} is missing mandatory **State:** metadata",
                        st.task_number, st.subtask_number
                    ),
                    None,
                )
            })?;
            t.subtasks.push(Subtask {
                task_number: st.task_number,
                subtask_number: st.subtask_number,
                title: st.title,
                state,
                content: st.content,
            });
        }
    }

    if let Some(tb) = cur_task.take() {
        let state = tb.state.ok_or_else(|| {
            ParseError::new(
                format!("Task {} is missing mandatory **State:** metadata", tb.id),
                None,
            )
        })?;
        tasks.push(Task {
            id: tb.id,
            title: tb.title,
            state,
            prior: tb.prior,
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

    Ok(Rhei {
        title,
        states: rhei_states.unwrap_or_else(|| "rhei".to_string()),
        metadata: rhei_metadata,
        content_sections: rhei_content,
        tasks,
    })
}

/// Parsed workspace index file (the root `index.rhei.md` of a directory workspace).
///
/// Contains the plan title, optional states declaration, and content sections,
/// but no tasks (those live in the `tasks/` subdirectory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIndex {
    pub title: String,
    pub states: String,
    pub metadata: Option<Metadata>,
    pub content_sections: Vec<ContentSection>,
}

/// Parse a workspace index file (`index.rhei.md`).
///
/// Expects `# Rhei: <title>`, optional `**States:**`, and content sections.
/// Returns an error if a `## Tasks` section is present (tasks belong in `tasks/`).
pub fn parse_workspace_index(input: &str) -> Result<WorkspaceIndex> {
    let re_rhei = Regex::new(r#"^#\s+Rhei:\s+(.*)$"#).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();

    let mut title: Option<String> = None;
    let mut states: Option<String> = None;
    let mut states_checked = false;
    let mut metadata: Option<Metadata> = None;
    let mut frontmatter_checked = false;
    let mut in_frontmatter = false;
    let mut frontmatter_start_line = 0usize;
    let mut frontmatter_lines: Vec<String> = Vec::new();
    let mut header_seen = false;
    let mut content: Vec<ContentSection> = Vec::new();
    let mut in_code_block = false;

    for (idx, raw) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw.trim();

        if in_frontmatter {
            if line == "---" {
                metadata = Some(parse_frontmatter(
                    &frontmatter_lines,
                    frontmatter_start_line,
                    "workspace index",
                )?);
                in_frontmatter = false;
                continue;
            }
            frontmatter_lines.push(raw.to_string());
            continue;
        }

        let trimmed_start = raw.trim_start();
        if trimmed_start.starts_with("```") {
            in_code_block = !in_code_block;
            if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
                if !c.is_empty() {
                    c.push('\n');
                }
                c.push_str(raw);
            }
            continue;
        }

        if in_code_block {
            if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
                if !c.is_empty() {
                    c.push('\n');
                }
                c.push_str(raw);
            }
            continue;
        }

        if line.is_empty() {
            continue;
        }

        if !header_seen {
            if let Some(cap) = re_rhei.captures(line) {
                title = Some(cap.get(1).unwrap().as_str().to_string());
                header_seen = true;
                continue;
            }
            let is_h1 = line.starts_with('#') && !line.starts_with("##");
            if is_h1 {
                return Err(ParseError::new(
                    "Malformed rhei heading: expected '# Rhei: <title>'",
                    Some(line_number),
                ));
            }
            continue;
        }

        // Check for **States:** declaration (must be first non-empty line after header)
        if !states_checked {
            if let Some(cap) = re_states_decl.captures(line) {
                states = Some(cap.get(1).unwrap().as_str().trim().to_string());
                states_checked = true;
                continue;
            }
            states_checked = true;
        }

        if states_checked && !frontmatter_checked {
            if line == "---" {
                in_frontmatter = true;
                frontmatter_checked = true;
                frontmatter_start_line = line_number + 1;
                frontmatter_lines.clear();
                continue;
            }
            frontmatter_checked = true;
        }

        if re_tasks.is_match(line) {
            return Err(ParseError::new(
                "Workspace index file must not contain a '## Tasks' section; tasks belong in the tasks/ directory",
                Some(line_number),
            ));
        }

        if let Some(cap) = re_section_header.captures(line) {
            let section_title = cap.get(1).unwrap().as_str().trim().to_string();
            content.push(ContentSection { title: section_title, content: String::new() });
            continue;
        }

        // Content line: append to current section if one is open.
        if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
            if !c.is_empty() {
                c.push('\n');
            }
            c.push_str(raw);
        }
        // Loose text outside a section is silently ignored per spec.
    }

    if in_frontmatter {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(frontmatter_start_line.saturating_sub(1).max(1)),
        ));
    }

    let title = title.ok_or_else(|| ParseError::new("Missing '# Rhei: <title>' header", None))?;

    Ok(WorkspaceIndex {
        title,
        states: states.unwrap_or_else(|| "rhei".to_string()),
        metadata,
        content_sections: content,
    })
}

/// Parse a workspace task file (a file inside the `tasks/` directory).
///
/// These files contain one or more `### Task <id>: <title>` entries directly,
/// without a `# Rhei:` header or `## Tasks` section.
pub fn parse_workspace_tasks(input: &str) -> Result<Vec<Task>> {
    // Prepend a synthetic header so the existing parser can handle the content.
    let prefix = "# Rhei: _workspace_\n\n## Tasks\n\n";
    let prefix_line_count = 4; // 4 lines in the prefix
    let synthetic = format!("{}{}", prefix, input);
    match parse(&synthetic) {
        Ok(rhei) => Ok(rhei.tasks),
        Err(mut e) => {
            if let Some(ref mut line) = e.line {
                *line = line.saturating_sub(prefix_line_count);
                if *line == 0 {
                    *line = 1;
                }
            }
            Err(e)
        }
    }
}

/// Unescape a state value: supports backtick wrapping only.
fn unescape_state(input: &str) -> String {
    if input.starts_with('`') && input.ends_with('`') && input.len() >= 2 {
        return input[1..input.len() - 1].to_string();
    }
    input.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ContentSection, TaskId};
    use serde_yaml::Value as YamlValue;

    fn yaml_key(name: &str) -> YamlValue {
        YamlValue::String(name.to_string())
    }

    #[test]
    fn parses_minimal_plan_with_task_and_subtasks() {
        let input = r#"# Rhei: Example
Some intro line

## Tasks

### Task 1: Alpha
**State:** pending
**Prior:** Task 2

#### Subtask 1.1: Do A
**State:** pending
Line A1
Line A2

#### Subtask 1.2: Do B
**State:** completed
```
code block
```
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.title, "Example");
        // "Some intro line" is loose text (not inside a section), silently ignored per spec.
        assert!(rhei.content_sections.is_empty());

        assert_eq!(rhei.tasks.len(), 1);
        let t1 = &rhei.tasks[0];
        assert!(matches!(t1.id, TaskId::Number(1)));
        assert_eq!(t1.title, "Alpha");
        assert_eq!(t1.state, "pending");
        assert_eq!(t1.prior, vec![TaskId::Number(2)]);

        assert_eq!(t1.subtasks.len(), 2);
        assert_eq!(t1.subtasks[0].title, "Do A");
        assert_eq!(t1.subtasks[0].state, "pending");
        assert!(t1.subtasks[0].content.contains("Line A1"));
        assert!(t1.subtasks[0].content.contains("Line A2"));

        assert_eq!(t1.subtasks[1].state, "completed");
        assert!(t1.subtasks[1].content.contains("```"));
        assert!(t1.subtasks[1].content.contains("code block"));
    }

    #[test]
    fn parses_plan_frontmatter_metadata() {
        let input = r#"# Rhei: Example

---
metadata:
  tasks:
    1:
      stateVisits:
        review: 2
---

## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        let metadata = rhei.metadata.expect("metadata should be present");
        let metadata_section = metadata
            .get(yaml_key("metadata"))
            .and_then(YamlValue::as_mapping)
            .expect("metadata section");
        let tasks = metadata_section
            .get(yaml_key("tasks"))
            .and_then(YamlValue::as_mapping)
            .expect("tasks metadata");
        let task = tasks
            .get(YamlValue::Number(1u64.into()))
            .and_then(YamlValue::as_mapping)
            .expect("task 1 metadata");
        let state_visits =
            task.get(yaml_key("stateVisits")).and_then(YamlValue::as_mapping).expect("stateVisits");

        assert_eq!(state_visits.get(yaml_key("review")).and_then(YamlValue::as_u64), Some(2));
    }

    #[test]
    fn parses_workspace_index_frontmatter_metadata() {
        let input = r#"# Rhei: Workspace
**States:** custom

---
metadata:
  tasks:
    setup:
      retryCount: 1
---

## Overview
Context
"#;

        let index = parse_workspace_index(input).expect("parse ok");
        assert_eq!(index.states, "custom");
        let metadata = index.metadata.expect("metadata should be present");
        let metadata_section = metadata
            .get(yaml_key("metadata"))
            .and_then(YamlValue::as_mapping)
            .expect("metadata section");
        let tasks = metadata_section
            .get(yaml_key("tasks"))
            .and_then(YamlValue::as_mapping)
            .expect("tasks metadata");
        let setup =
            tasks.get(yaml_key("setup")).and_then(YamlValue::as_mapping).expect("setup metadata");

        assert_eq!(setup.get(yaml_key("retryCount")).and_then(YamlValue::as_u64), Some(1));
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
            rhei.content_sections,
            vec![
                ContentSection {
                    title: "Overview".to_string(),
                    content: "High-level context.".to_string(),
                },
                ContentSection {
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
**State:** pending
Body
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.tasks.len(), 1);
        let task = &rhei.tasks[0];
        assert_eq!(task.id, TaskId::Named("build_api".to_string()));
        assert_eq!(task.title, "Build API");
        assert_eq!(task.state, "in-progress");
        assert_eq!(task.prior, vec![TaskId::Named("setup_db".to_string()), TaskId::Number(2)]);
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
**State:** pending
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
    fn prior_before_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Prior:** Task 2
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert!(err.message.contains("**State:** must appear before **Prior:**"));
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
        // Loose text and code fences outside sections are silently ignored.
        assert!(rhei.content_sections.is_empty());
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
**State:** pending
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
**State:** pending
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
