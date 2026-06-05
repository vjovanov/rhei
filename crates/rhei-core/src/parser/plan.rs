use crate::ast::{ContentSection, Metadata, Rhei, Structure, Task, MAX_ALLOWED_LEVELS};
use crate::text::parse_task_id;
use regex::Regex;

use super::builder::{title_case_kind, unwind_to_level, NodeBuilder};
use super::{parse_frontmatter, parse_structure, unescape_state, ParseError, Result};

pub fn parse(input: &str) -> Result<Rhei> {
    let re_rhei = Regex::new(r#"^#\s+Rhei:\s+(.*)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let task_id_segment = r#"(?:[A-Za-z][A-Za-z0-9_-]*|0|[1-9][0-9]*)"#;
    let task_id_pattern = format!(r#"{task_id_segment}(?:\.{task_id_segment})*"#);
    let re_node_header = Regex::new(&format!(
        r#"^(#{{3,6}})\s+([A-Za-z][A-Za-z0-9_-]*)\s+({task_id_pattern}):\s+(.*)$"#
    ))
    .unwrap();
    let re_any_h3_to_h6 = Regex::new(r#"^#{3,6}\s+\S.*$"#).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();
    let re_state_like = Regex::new(r#"^\*\*State\b.*$"#).unwrap();
    let re_prior_ref =
        Regex::new(&format!(r#"^([A-Za-z][A-Za-z0-9_-]*)\s+({task_id_pattern})$"#)).unwrap();
    let re_prior_like = Regex::new(r#"^\*\*Prior\b.*$"#).unwrap();
    let re_assignee = Regex::new(r#"^\*\*Assignee:\*\*\s*(.+)$"#).unwrap();
    let re_assignee_like = Regex::new(r#"^\*\*Assignee\b.*$"#).unwrap();
    let re_model = Regex::new(r#"^\*\*Model:\*\*\s*(.+)$"#).unwrap();
    let re_model_like = Regex::new(r#"^\*\*Model(?::\*\*\s*|\*\*.*)$"#).unwrap();
    let re_target = Regex::new(r#"^\*\*Target:\*\*\s*(.+)$"#).unwrap();
    let re_target_like = Regex::new(r#"^\*\*Target(?::\*\*\s*|\*\*.*)$"#).unwrap();
    let re_h2_heading = Regex::new(r#"^##\s+\S.*$"#).unwrap();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();

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
    let mut tasks: Vec<Task> = Vec::new();
    let mut node_stack: Vec<NodeBuilder> = Vec::new();

    // Structure is finalised once the frontmatter boundary is passed.
    let mut structure: Structure = Structure::default();
    let mut structure_finalised = false;

    for (idx, raw) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw.trim();

        if !line.is_empty() {
            first_nonempty_line.get_or_insert(line_number);
        }

        if in_frontmatter {
            if line == "---" {
                let metadata =
                    parse_frontmatter(&frontmatter_lines, frontmatter_start_line, "plan")?;
                structure = parse_structure(Some(&metadata), frontmatter_start_line)?;
                structure_finalised = true;
                rhei_metadata = Some(metadata);
                in_frontmatter = false;
                continue;
            }
            frontmatter_lines.push(raw.to_string());
            continue;
        }

        let trimmed_start = raw.trim_start();
        let is_fence = trimmed_start.starts_with("```");
        if is_fence {
            in_code_block = !in_code_block;

            if in_tasks_section {
                if let Some(top) = node_stack.last_mut() {
                    top.metadata_closed = true;
                    if !top.content.is_empty() {
                        top.content.push('\n');
                    }
                    top.content.push_str(raw);
                }
            } else if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            }
            continue;
        }

        if line.is_empty() {
            if in_tasks_section {
                if let Some(top) = node_stack.last_mut() {
                    top.content.push('\n');
                }
            } else if in_code_block {
                if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                    content.push('\n');
                }
            }
            continue;
        }

        if !rhei_header_seen && !in_code_block && line == "---" {
            return Err(ParseError::new(
                "YAML frontmatter must appear after the `# Rhei:` header (and any `**States:**` declaration). Move the `---` block below the header.",
                Some(line_number),
            ));
        }

        if !in_tasks_section && !in_code_block {
            if let Some(cap) = re_rhei.captures(line) {
                rhei_title = Some(cap.get(1).unwrap().as_str().to_string());
                rhei_header_seen = true;
                continue;
            }

            if rhei_header_seen && !rhei_states_checked {
                if let Some(cap) = re_states_decl.captures(line) {
                    rhei_states = Some(cap.get(1).unwrap().as_str().trim().to_string());
                    rhei_states_checked = true;
                    continue;
                }
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

            if !structure_finalised && frontmatter_checked {
                // No frontmatter block was opened — apply defaults.
                structure_finalised = true;
            }

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

            if re_state_like.is_match(line)
                || re_prior_like.is_match(line)
                || re_assignee_like.is_match(line)
                || re_model_like.is_match(line)
                || re_target_like.is_match(line)
            {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            }

            if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            }
            continue;
        }

        if in_code_block {
            if let Some(top) = node_stack.last_mut() {
                if !top.content.is_empty() {
                    top.content.push('\n');
                }
                top.content.push_str(raw);
            }
            continue;
        }

        if let Some(caps) = re_node_header.captures(line) {
            let hashes = caps.get(1).unwrap().as_str();
            let level = hashes.len() as u8;
            let kind_raw = caps.get(2).unwrap().as_str();
            let id_str = caps.get(3).unwrap().as_str();
            let title = caps.get(4).unwrap().as_str().to_string();

            let kind_canonical = kind_raw.to_ascii_lowercase();
            if !structure.accepts_kind(&kind_canonical) {
                return Err(ParseError::new(
                    format!(
                        "unknown node kind `{}` at heading; declared kinds are {:?}",
                        kind_raw, structure.node_kinds
                    ),
                    Some(line_number),
                ));
            }

            let depth = level.saturating_sub(2); // H3 -> depth 1
            if depth == 0 || depth > MAX_ALLOWED_LEVELS {
                return Err(ParseError::new(
                    format!("node heading depth out of range at level {level}"),
                    Some(line_number),
                ));
            }
            if depth > structure.max_levels {
                return Err(ParseError::new(
                    format!(
                        "node depth {depth} exceeds `structure.maxLevels` ({})",
                        structure.max_levels
                    ),
                    Some(line_number),
                ));
            }

            let id = parse_task_id(id_str).ok_or_else(|| {
                ParseError::new(
                    format!("malformed task id `{id_str}` in node heading"),
                    Some(line_number),
                )
            })?;

            if id.depth() as u8 != depth {
                return Err(ParseError::new(
                    format!(
                        "heading depth {depth} does not match id path depth {}; \
                         level-{level} nodes require {depth}-segment ids",
                        id.depth()
                    ),
                    Some(line_number),
                ));
            }

            if title.trim().is_empty() {
                return Err(ParseError::new(
                    format!(
                        "malformed node heading: expected '{} {} <id>: <title>'",
                        "#".repeat(level as usize),
                        title_case_kind(&kind_canonical)
                    ),
                    Some(line_number),
                ));
            }

            // Finalise any open nodes at >= this depth and attach them.
            unwind_to_level(&mut node_stack, &mut tasks, level)?;

            if depth > 1 {
                let Some(parent) = node_stack.last() else {
                    return Err(ParseError::new(
                        format!("level-{level} node `{id}` has no enclosing parent task"),
                        Some(line_number),
                    ));
                };
                if !id.extends(&parent.id) {
                    return Err(ParseError::new(
                        format!(
                            "child id `{id}` must extend parent id `{}` by exactly one segment",
                            parent.id
                        ),
                        Some(line_number),
                    ));
                }
            }

            node_stack.push(NodeBuilder {
                id,
                kind: kind_canonical,
                title,
                level,
                state: None,
                prior: Vec::new(),
                assignee: None,
                model: None,
                target: None,
                content: String::new(),
                children: Vec::new(),
                metadata_closed: false,
                heading_line: line_number,
            });
            continue;
        }

        if re_any_h3_to_h6.is_match(line) {
            return Err(ParseError::new(
                "Malformed node heading: expected '### <Kind> <id>: <title>' (for example `### Task 1: Title`)",
                Some(line_number),
            ));
        }

        // **State:** metadata
        if let Some(caps) = re_state.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            let raw_state = caps.get(1).unwrap().as_str().trim();
            top.state = Some(unescape_state(raw_state));
            continue;
        }

        if re_state_like.is_match(line) {
            if node_stack.last().is_some() {
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

        // **Prior:** metadata
        if line.starts_with("**Prior:**") {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Prior:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            let prior_value = line.strip_prefix("**Prior:**").unwrap_or_default();
            let mut ids = Vec::new();
            for item in prior_value.split(',') {
                let item = item.trim();
                let Some(caps) = re_prior_ref.captures(item) else {
                    return Err(ParseError::new(
                        "Malformed metadata field: expected '**Prior:** Task <id>'",
                        Some(line_number),
                    ));
                };
                let Some(id) = caps.get(2).and_then(|m| parse_task_id(m.as_str())) else {
                    return Err(ParseError::new(
                        "Malformed metadata field: expected '**Prior:** Task <id>'",
                        Some(line_number),
                    ));
                };
                ids.push(id);
            }
            if ids.is_empty() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Prior:** Task <id>'",
                    Some(line_number),
                ));
            }
            top.prior.extend(ids);
            continue;
        }

        if re_prior_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Prior:** Task <id>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Assignee:** metadata
        if let Some(caps) = re_assignee.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Assignee:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.assignee.is_some() {
                return Err(ParseError::new(
                    format!("Duplicate **Assignee:** metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            top.assignee = Some(caps.get(1).unwrap().as_str().trim().to_string());
            continue;
        }

        if re_assignee_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Assignee:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Model:** metadata
        if let Some(caps) = re_model.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Model:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.model.is_some() {
                return Err(ParseError::new(
                    format!("Duplicate **Model:** metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            top.model = Some(caps.get(1).unwrap().as_str().trim().to_string());
            continue;
        }

        if re_model_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Model:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Target:** metadata
        if let Some(caps) = re_target.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Target:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.target.is_some() {
                return Err(ParseError::new(
                    format!("Duplicate **Target:** metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            top.target = Some(caps.get(1).unwrap().as_str().trim().to_string());
            continue;
        }

        if re_target_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Target:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        if re_h2_heading.is_match(line) {
            return Err(ParseError::new(
                "Tasks section must be the final '##' chapter and appear as '## Tasks'",
                Some(line_number),
            ));
        }

        // Content line: append to the innermost open node.
        if let Some(top) = node_stack.last_mut() {
            top.metadata_closed = true;
            if !top.content.is_empty() {
                top.content.push('\n');
            }
            top.content.push_str(raw);
        }
    }

    if in_frontmatter {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(frontmatter_start_line.saturating_sub(1).max(1)),
        ));
    }

    // Finalise remaining open nodes.
    unwind_to_level(&mut node_stack, &mut tasks, 0)?;

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

    let states_declared = rhei_states.is_some();
    Ok(Rhei {
        title,
        states: rhei_states.unwrap_or_else(|| "rhei".to_string()),
        states_declared,
        structure,
        metadata: rhei_metadata,
        content_sections: rhei_content,
        tasks,
    })
}
