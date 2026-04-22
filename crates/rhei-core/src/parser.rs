//! Recursive-descent parser for Markdown plans.
//!
//! Responsibilities:
//! - Extract the rhei title and pre-Tasks content sections.
//! - Parse YAML frontmatter, including the plan-level `structure` block
//!   (`maxLevels`, `nodeKinds`).
//! - Build the recursive task tree from H3..=H6 node headings, using the
//!   configured `nodeKinds` to accept the leading keyword.
//! - Attach child nodes to parents based on id-segment extension and heading
//!   depth.

use crate::ast::{
    ContentSection, Metadata, Rhei, Structure, Task, TaskId, DEFAULT_MAX_LEVELS,
    DEFAULT_NODE_KIND, MAX_ALLOWED_LEVELS,
};
use crate::text::parse_task_id;
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

pub type Result<T> = std::result::Result<T, ParseError>;

/// Extract the plan-level `structure` block from parsed frontmatter metadata.
///
/// Returns the default `Structure` when the block is absent. Validates shape
/// (types, ranges, uniqueness) and reports parse errors inline.
fn parse_structure(metadata: Option<&Metadata>, start_line: usize) -> Result<Structure> {
    let Some(metadata) = metadata else {
        return Ok(Structure::default());
    };
    let structure_key = YamlValue::String("structure".to_string());
    let Some(block) = metadata.get(&structure_key) else {
        return Ok(Structure::default());
    };
    let mapping = block.as_mapping().ok_or_else(|| {
        ParseError::new("plan frontmatter `structure` must be a mapping", Some(start_line))
    })?;

    let mut max_levels: u8 = DEFAULT_MAX_LEVELS;
    if let Some(v) = mapping.get(&YamlValue::String("maxLevels".to_string())) {
        let n = v.as_u64().ok_or_else(|| {
            ParseError::new(
                "plan frontmatter `structure.maxLevels` must be a positive integer",
                Some(start_line),
            )
        })?;
        if n == 0 || n > MAX_ALLOWED_LEVELS as u64 {
            return Err(ParseError::new(
                format!(
                    "plan frontmatter `structure.maxLevels` must be in 1..={}",
                    MAX_ALLOWED_LEVELS
                ),
                Some(start_line),
            ));
        }
        max_levels = n as u8;
    }

    let mut node_kinds: Vec<String> = vec![DEFAULT_NODE_KIND.to_string()];
    if let Some(v) = mapping.get(&YamlValue::String("nodeKinds".to_string())) {
        let seq = v.as_sequence().ok_or_else(|| {
            ParseError::new(
                "plan frontmatter `structure.nodeKinds` must be a sequence of strings",
                Some(start_line),
            )
        })?;
        if seq.is_empty() {
            return Err(ParseError::new(
                "plan frontmatter `structure.nodeKinds` must not be empty",
                Some(start_line),
            ));
        }
        let mut out = Vec::with_capacity(seq.len());
        for entry in seq {
            let name = entry.as_str().ok_or_else(|| {
                ParseError::new(
                    "plan frontmatter `structure.nodeKinds` entries must be strings",
                    Some(start_line),
                )
            })?;
            let canonical = name.trim().to_ascii_lowercase();
            if canonical.is_empty() {
                return Err(ParseError::new(
                    "plan frontmatter `structure.nodeKinds` entries must be non-empty",
                    Some(start_line),
                ));
            }
            if canonical == "rhei" {
                return Err(ParseError::new(
                    "`rhei` is a reserved node kind and must not appear in `structure.nodeKinds`",
                    Some(start_line),
                ));
            }
            if out.iter().any(|k: &String| k == &canonical) {
                return Err(ParseError::new(
                    format!(
                        "plan frontmatter `structure.nodeKinds` contains duplicate entry `{canonical}`"
                    ),
                    Some(start_line),
                ));
            }
            out.push(canonical);
        }
        node_kinds = out;
    }

    Ok(Structure { max_levels, node_kinds })
}

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

/// In-progress builder for a node. The stack holds one builder per open
/// heading level; the innermost builder is the current parse target.
struct NodeBuilder {
    id: TaskId,
    kind: String,
    title: String,
    level: u8,
    state: Option<String>,
    prior: Vec<TaskId>,
    assignee: Option<String>,
    content: String,
    children: Vec<Task>,
    /// Once non-metadata content appears, further metadata fields become
    /// errors.
    metadata_closed: bool,
    /// Line number of the heading (for error reporting).
    heading_line: usize,
}

fn finalize_builder(b: NodeBuilder) -> Result<Task> {
    let state = b.state.ok_or_else(|| {
        ParseError::new(
            format!("{} {} is missing mandatory **State:** metadata", title_case_kind(&b.kind), b.id),
            Some(b.heading_line),
        )
    })?;
    Ok(Task {
        id: b.id,
        kind: b.kind,
        title: b.title,
        state,
        prior: b.prior,
        assignee: b.assignee,
        content: b.content,
        children: b.children,
    })
}

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

/// Pop builders from the stack until its top is at depth `< level`, attaching
/// each popped builder to its parent (or to the root task list when popping
/// the outermost).
fn unwind_to_level(
    stack: &mut Vec<NodeBuilder>,
    tasks: &mut Vec<Task>,
    target_level: u8,
) -> Result<()> {
    while let Some(top) = stack.last() {
        if top.level < target_level {
            break;
        }
        let popped = stack.pop().expect("stack was non-empty");
        let finished = finalize_builder(popped)?;
        if let Some(parent) = stack.last_mut() {
            parent.children.push(finished);
        } else {
            tasks.push(finished);
        }
    }
    Ok(())
}

/// Parse a markdown plan, returning the successfully parsed [`Rhei`] (if any)
/// together with every recoverable error discovered along the way.
///
/// Recoverable errors are per-task issues like a missing `**State:**` line,
/// a malformed metadata field, or an out-of-order `**Prior:**`/`**Assignee:**`.
/// When one is seen the offending line (or the containing task block) is
/// stripped from the input and parsing is retried. Fatal structural errors
/// (missing `# Rhei:` header, unterminated YAML frontmatter, missing
/// `## Tasks` section, circular parent/child references) still bail — but
/// come back through the same return channel so callers can enumerate the
/// full list.
///
/// The single-error [`parse`] wrapper still exists for callers that only
/// care about the first problem.
pub fn parse_collect(input: &str) -> (Option<Rhei>, Vec<ParseError>) {
    let mut errors: Vec<ParseError> = Vec::new();
    let mut working = input.to_string();

    // Safety valve in case a recovery step fails to make progress: cap
    // iterations at a generous multiple of the authored line count.
    let max_iters = input.lines().count().saturating_mul(2).max(16);

    for _ in 0..max_iters {
        match parse(&working) {
            Ok(rhei) => return (Some(rhei), errors),
            Err(err) => {
                // Only recover from well-understood per-task errors. For
                // structural issues (missing header, unknown node kind,
                // depth mismatches) stop immediately — continuing would
                // produce cascading, misleading error lists.
                let recoverable = is_recoverable_error(&err.message);
                let Some(line) = err.line.filter(|_| recoverable) else {
                    errors.push(err);
                    return (None, errors);
                };
                let (stripped, made_progress) = strip_for_recovery(&working, line, &err.message);
                errors.push(err);
                if !made_progress {
                    return (None, errors);
                }
                working = stripped;
            }
        }
    }

    (None, errors)
}

/// Return true when an error is safe to skip past and continue parsing.
///
/// Safe-to-recover errors are strictly local to a task or a single
/// metadata line. Structural errors (missing plan header, mismatched
/// heading depth, tasks out of order) cascade and produce misleading
/// secondary errors when we try to strip them.
fn is_recoverable_error(message: &str) -> bool {
    const RECOVERABLE_MARKERS: &[&str] = &[
        "missing mandatory **State:**",
        "Malformed metadata field",
        "Metadata field appears outside a task",
        "Metadata fields must appear immediately",
        "**State:** must appear before **Prior:**",
        "**State:** must appear before **Assignee:**",
        "Duplicate **Assignee:**",
    ];
    RECOVERABLE_MARKERS.iter().any(|m| message.contains(m))
}

/// Rewrite the plan source so the next `parse()` attempt skips past the
/// offending line or task. Returns the new source plus a flag that is
/// `false` when recovery is not possible (the caller should stop).
///
/// Recovery refuses to strip anything that would leave the plan with no
/// tasks at all, since doing so would cascade into "Tasks section must
/// contain at least one task" — an artifact of the recovery, not a real
/// authoring mistake.
fn strip_for_recovery(input: &str, line: usize, message: &str) -> (String, bool) {
    if line == 0 {
        return (input.to_string(), false);
    }

    // Patterns that indicate the whole surrounding task is unrecoverable and
    // should be dropped. Each task is `### Task <id>: ...` through the next
    // H3..=H6 heading (exclusive).
    //
    // Dropping only the `**State:**` line creates a cascading "missing
    // mandatory **State:**" error on the next pass for the same task, which
    // is spurious from the user's point of view. Treat state-related
    // malformed metadata as a whole-task issue.
    let whole_task_markers = [
        "missing mandatory **State:**",
        "malformed node heading",
        "expected '**State:** <value>'",
    ];
    let drop_whole_task = whole_task_markers.iter().any(|m| message.contains(m));

    let lines: Vec<&str> = input.lines().collect();
    if line > lines.len() {
        return (input.to_string(), false);
    }

    let idx = line - 1;
    let heading_re = regex::Regex::new(r#"^#{3,6}\s+[A-Za-z][A-Za-z0-9_-]*\s+"#).expect("regex");

    if drop_whole_task {
        let total_tasks = lines.iter().filter(|l| heading_re.is_match(l)).count();
        if total_tasks <= 1 {
            // Stripping would empty the Tasks section, which cascades into
            // secondary errors. Bail instead.
            return (input.to_string(), false);
        }

        // Walk backwards to find the task heading that owns this line.
        let mut start = idx;
        loop {
            if heading_re.is_match(lines[start]) {
                break;
            }
            if start == 0 {
                return (input.to_string(), false);
            }
            start -= 1;
        }
        let mut end = start + 1;
        while end < lines.len() && !heading_re.is_match(lines[end]) {
            end += 1;
        }
        if start == 0 && end == lines.len() {
            return (input.to_string(), false);
        }
        let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
        kept.extend_from_slice(&lines[..start]);
        kept.extend_from_slice(&lines[end..]);
        let mut out = kept.join("\n");
        if input.ends_with('\n') {
            out.push('\n');
        }
        return (out, true);
    }

    // Default recovery: drop just the offending line.
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len().saturating_sub(1));
    kept.extend_from_slice(&lines[..idx]);
    if idx + 1 < lines.len() {
        kept.extend_from_slice(&lines[idx + 1..]);
    }
    let mut out = kept.join("\n");
    if input.ends_with('\n') {
        out.push('\n');
    }
    (out, true)
}

/// Parse a markdown plan into a Rhei AST.
pub fn parse(input: &str) -> Result<Rhei> {
    let re_rhei = Regex::new(r#"^#\s+Rhei:\s+(.*)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_node_header = Regex::new(
        r#"^(#{3,6})\s+([A-Za-z][A-Za-z0-9_-]*)\s+((?:[A-Za-z][A-Za-z0-9_-]*|[0-9]+)(?:\.(?:[A-Za-z][A-Za-z0-9_-]*|[0-9]+))*):\s+(.*)$"#,
    )
    .unwrap();
    let re_any_h3_to_h6 = Regex::new(r#"^#{3,6}\s+\S.*$"#).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();
    let re_state_like = Regex::new(r#"^\*\*State\b.*$"#).unwrap();
    let re_prior_ref = Regex::new(
        r#"([A-Za-z][A-Za-z0-9_-]*)\s+((?:[A-Za-z][A-Za-z0-9_-]*|[0-9]+)(?:\.(?:[A-Za-z][A-Za-z0-9_-]*|[0-9]+))*)"#,
    )
    .unwrap();
    let re_prior_like = Regex::new(r#"^\*\*Prior\b.*$"#).unwrap();
    let re_assignee = Regex::new(r#"^\*\*Assignee:\*\*\s*(.+)$"#).unwrap();
    let re_assignee_like = Regex::new(r#"^\*\*Assignee\b.*$"#).unwrap();
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

        // In the tasks section.
        if in_code_block {
            if let Some(top) = node_stack.last_mut() {
                if !top.content.is_empty() {
                    top.content.push('\n');
                }
                top.content.push_str(raw);
            }
            continue;
        }

        // Node heading (H3..=H6)?
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
            let ids: Vec<TaskId> = re_prior_ref
                .captures_iter(line)
                .filter_map(|c| c.get(2))
                .filter_map(|m| parse_task_id(m.as_str()))
                .collect();
            top.prior.extend(ids);
            continue;
        }

        if re_prior_like.is_match(line) {
            if node_stack.last().is_some() {
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

    Ok(Rhei {
        title,
        states: rhei_states.unwrap_or_else(|| "rhei".to_string()),
        structure,
        metadata: rhei_metadata,
        content_sections: rhei_content,
        tasks,
    })
}

/// Parsed workspace index file (the root `index.rhei.md` of a directory workspace).
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
    use crate::ast::{ContentSection, TaskId, TaskIdSegment};
    use serde_yaml::Value as YamlValue;

    fn yaml_key(name: &str) -> YamlValue {
        YamlValue::String(name.to_string())
    }

    #[test]
    fn parses_minimal_plan_with_hierarchical_tasks() {
        let input = r#"# Rhei: Example
Some intro line

## Tasks

### Task 1: Alpha
**State:** pending
**Prior:** Task 2

#### Task 1.1: Do A
**State:** pending
Line A1
Line A2

#### Task 1.2: Do B
**State:** completed
```
code block
```
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.title, "Example");
        assert!(rhei.content_sections.is_empty());

        assert_eq!(rhei.tasks.len(), 1);
        let t1 = &rhei.tasks[0];
        assert_eq!(t1.kind, "task");
        assert_eq!(t1.id, TaskId::number(1));
        assert_eq!(t1.title, "Alpha");
        assert_eq!(t1.state, "pending");
        assert_eq!(t1.prior, vec![TaskId::number(2)]);

        assert_eq!(t1.children.len(), 2);
        assert_eq!(t1.children[0].title, "Do A");
        assert_eq!(t1.children[0].state, "pending");
        assert_eq!(
            t1.children[0].id,
            TaskId::from_segments(vec![TaskIdSegment::Number(1), TaskIdSegment::Number(1)])
        );
        assert!(t1.children[0].content.contains("Line A1"));
        assert!(t1.children[0].content.contains("Line A2"));

        assert_eq!(t1.children[1].state, "completed");
        assert!(t1.children[1].content.contains("```"));
        assert!(t1.children[1].content.contains("code block"));
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
    fn parses_structure_frontmatter_with_custom_node_kinds_and_depth() {
        let input = r#"# Rhei: Example
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Parent
**State:** pending

#### Bug 1.1: Child
**State:** pending

##### Task 1.1.1: Grandchild
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        assert_eq!(rhei.structure.max_levels, 3);
        assert_eq!(rhei.structure.node_kinds, vec!["task".to_string(), "bug".to_string()]);
        assert_eq!(rhei.tasks.len(), 1);
        assert_eq!(rhei.tasks[0].children.len(), 1);
        assert_eq!(rhei.tasks[0].children[0].kind, "bug");
        assert_eq!(rhei.tasks[0].children[0].children.len(), 1);
        assert_eq!(rhei.tasks[0].children[0].children[0].kind, "task");
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

#### Task build_api.endpoint: Implement endpoint
**State:** pending
Body
"#;

        let rhei = parse(input).expect("parse ok");

        assert_eq!(rhei.tasks.len(), 1);
        let task = &rhei.tasks[0];
        assert_eq!(task.id, TaskId::named("build_api"));
        assert_eq!(task.title, "Build API");
        assert_eq!(task.state, "in-progress");
        assert_eq!(task.prior, vec![TaskId::named("setup_db"), TaskId::number(2)]);
        assert_eq!(task.children.len(), 1);
        assert_eq!(
            task.children[0].id,
            TaskId::from_segments(vec![
                TaskIdSegment::Named("build_api".to_string()),
                TaskIdSegment::Named("endpoint".to_string())
            ])
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
    fn errors_on_malformed_task_heading_in_tasks_section() {
        // `### Tak 3:` parses as kind `Tak`, which is not declared.
        let input = r#"# Rhei: Example
## Tasks

### Tak 3: Broken heading
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert!(err.message.contains("unknown node kind"));
        assert_eq!(err.line, Some(4));
    }

    #[test]
    fn errors_on_unknown_node_kind() {
        let input = r#"# Rhei: Example
## Tasks

### Spike 1: Investigate
**State:** pending
"#;
        let err = parse(input).unwrap_err();
        assert!(err.message.contains("unknown node kind"));
    }

    #[test]
    fn errors_on_child_id_that_does_not_extend_parent() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

#### Task 2.1: Wrong parent
**State:** pending
"#;
        let err = parse(input).unwrap_err();
        assert!(err.message.contains("must extend parent id"));
    }

    #[test]
    fn parses_assignee_when_present() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** in-progress
**Prior:** Task 2
**Assignee:** alice
Body
"#;

        let rhei = parse(input).expect("parse ok");
        let task = &rhei.tasks[0];
        assert_eq!(task.state, "in-progress");
        assert_eq!(task.prior, vec![TaskId::number(2)]);
        assert_eq!(task.assignee.as_deref(), Some("alice"));
        assert!(task.content.contains("Body"));
    }

    #[test]
    fn parses_assignee_without_prior() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Assignee:** bob
"#;

        let rhei = parse(input).expect("parse ok");
        assert_eq!(rhei.tasks[0].prior, Vec::<TaskId>::new());
        assert_eq!(rhei.tasks[0].assignee.as_deref(), Some("bob"));
    }

    #[test]
    fn parses_task_without_assignee_leaves_none() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let rhei = parse(input).expect("parse ok");
        assert_eq!(rhei.tasks[0].assignee, None);
    }

    #[test]
    fn errors_when_assignee_before_state() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Assignee:** alice
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("**State:** must appear before **Assignee:**"),
            "unexpected message: {}",
            err.message
        );
    }

    #[test]
    fn errors_when_duplicate_assignee() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Assignee:** alice
**Assignee:** bob
"#;

        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("Duplicate **Assignee:**"),
            "unexpected message: {}",
            err.message
        );
    }

    #[test]
    fn errors_when_assignee_after_content() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
Body line closes metadata window.
**Assignee:** alice
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(
            err.message,
            "Metadata fields must appear immediately after the task heading before task content"
        );
    }

    #[test]
    fn errors_when_assignee_outside_task() {
        let input = r#"# Rhei: Example

**Assignee:** alice

## Tasks

### Task 1: Alpha
**State:** pending
"#;

        let err = parse(input).unwrap_err();
        assert_eq!(err.message, "Metadata field appears outside a task");
    }

    #[test]
    fn errors_on_depth_over_structure_max_levels() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

#### Task 1.1: Beta
**State:** pending

##### Task 1.1.1: Too deep
**State:** pending
"#;
        let err = parse(input).unwrap_err();
        assert!(err.message.contains("exceeds `structure.maxLevels`"));
    }
}
