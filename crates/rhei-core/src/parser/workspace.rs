use crate::ast::{ContentSection, Metadata, Structure, Task};
use regex::Regex;

use super::{parse, parse_frontmatter, parse_structure, ParseError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIndex {
    pub title: String,
    pub states: String,
    pub structure: Structure,
    pub metadata: Option<Metadata>,
    pub content_sections: Vec<ContentSection>,
}

/// Parse a workspace index file (`index.rhei.md`).
pub fn parse_workspace_index(input: &str) -> Result<WorkspaceIndex> {
    let re_rhei = Regex::new(r#"^#\s+Rhei:\s+(.*)$"#).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();

    let mut title: Option<String> = None;
    let mut states: Option<String> = None;
    let mut states_checked = false;
    let mut metadata: Option<Metadata> = None;
    let mut structure: Structure = Structure::default();
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
                let parsed = parse_frontmatter(
                    &frontmatter_lines,
                    frontmatter_start_line,
                    "workspace index",
                )?;
                structure = parse_structure(Some(&parsed), frontmatter_start_line)?;
                metadata = Some(parsed);
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

        if !header_seen && line == "---" {
            return Err(ParseError::new(
                "YAML frontmatter must appear after the `# Rhei:` header (and any `**States:**` declaration). Move the `---` block below the header.",
                Some(line_number),
            ));
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

        if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
            if !c.is_empty() {
                c.push('\n');
            }
            c.push_str(raw);
        }
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
        structure,
        metadata,
        content_sections: content,
    })
}

/// Parse a workspace task file (a file inside the `tasks/` directory).
pub fn parse_workspace_tasks(input: &str) -> Result<Vec<Task>> {
    let prefix = "# Rhei: _workspace_\n\n## Tasks\n\n";
    let prefix_line_count = 4;
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
