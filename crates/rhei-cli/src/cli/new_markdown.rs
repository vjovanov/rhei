// The markdown `rhei new` emits, and where it is inserted.
//
// Its own part because emitting a node and splicing it into an authored file
// are text concerns with no knowledge of ids, machines, or projects.

// §FS-rhei-new.2 §FS-rhei-new.3

/// Header fields shared by a single-file rhei and a workspace index.
struct RheiHeader<'a> {
    title: &'a str,
    states: Option<&'a str>,
    max_levels: Option<u8>,
    node_kinds: &'a [String],
    description: Option<&'a str>,
}

/// Render a rhei's header in the order the plan language fixes: heading,
/// `**States:**`, frontmatter, description. `with_tasks_section` appends the
/// `## Tasks` heading a single-file rhei requires and a workspace index must
/// not have.
// §FS-rhei-plan-language.1.1 §FS-rhei-plan-language.1.2
fn render_rhei_file(header: &RheiHeader<'_>, with_tasks_section: bool) -> String {
    let mut out = format!("# Rhei: {}\n", header.title.trim());
    if let Some(states) = header.states {
        out.push_str(&format!("**States:** {}\n", states.trim()));
    }
    if header.max_levels.is_some() || !header.node_kinds.is_empty() {
        out.push_str("\n---\nstructure:\n");
        if let Some(max_levels) = header.max_levels {
            out.push_str(&format!("  maxLevels: {max_levels}\n"));
        }
        if !header.node_kinds.is_empty() {
            let kinds: Vec<String> =
                header.node_kinds.iter().map(|kind| kind.trim().to_ascii_lowercase()).collect();
            out.push_str(&format!("  nodeKinds: [{}]\n", kinds.join(", ")));
        }
        out.push_str("---\n");
    }
    if let Some(description) = description_body(header.description) {
        out.push('\n');
        out.push_str(&description);
        out.push('\n');
    }
    if with_tasks_section {
        out.push_str("\n## Tasks\n");
    }
    out
}

/// Every metadata field a new ticket can carry, in plan-language order.
// §FS-rhei-plan-language.2: metadata = state, prior, provides, consumes,
// assignee, execution override.
struct TicketFields<'a> {
    kind: &'a str,
    local_id: &'a str,
    title: &'a str,
    state: &'a str,
    prior: &'a [String],
    provides: &'a [String],
    consumes: &'a [String],
    assignee: Option<&'a str>,
    model: Option<&'a str>,
    target: Option<&'a str>,
    description: Option<&'a str>,
}

/// Render one ticket node. The heading level follows the id depth, so a
/// rhei-local `1.2` is a `####` under its parent's `###`.
fn render_ticket(fields: &TicketFields<'_>) -> String {
    let depth = fields.local_id.split('.').count();
    let hashes = "#".repeat(depth + 2);
    let mut out = format!(
        "{hashes} {} {}: {}\n**State:** {}\n",
        title_case_kind(fields.kind),
        fields.local_id,
        fields.title.trim(),
        fields.state
    );
    for (label, values) in [
        ("Prior", fields.prior),
        ("Provides", fields.provides),
        ("Consumes", fields.consumes),
    ] {
        if !values.is_empty() {
            let joined =
                values.iter().map(|value| value.trim()).collect::<Vec<_>>().join(", ");
            out.push_str(&format!("**{label}:** {joined}\n"));
        }
    }
    for (label, value) in
        [("Assignee", fields.assignee), ("Model", fields.model), ("Target", fields.target)]
    {
        if let Some(value) = value {
            out.push_str(&format!("**{label}:** {}\n", value.trim()));
        }
    }
    if let Some(description) = description_body(fields.description) {
        out.push('\n');
        out.push_str(&description);
        out.push('\n');
    }
    out
}

/// Trim a description to a body worth writing, or `None` when it is blank.
fn description_body(description: Option<&str>) -> Option<String> {
    let body = description?.trim();
    (!body.is_empty()).then(|| body.to_string())
}

/// Append a ticket at the end of a file, separated by exactly one blank line.
/// This is where a top-level ticket goes: `## Tasks` is the final `##` chapter,
/// so the end of the file is the end of the section. §FS-rhei-new.3.1
fn append_ticket(raw: &str, block: &str) -> String {
    let mut out = raw.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block.trim_end_matches('\n'));
    out.push('\n');
    out
}

/// Insert a ticket immediately after `parent_local_id`'s existing subtree, so
/// the file stays in id order. `None` when the parent's heading is not in this
/// file. §FS-rhei-new.3.1
fn insert_ticket_after_subtree(raw: &str, parent_local_id: &str, block: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut in_code_block = false;
    let mut parent: Option<(usize, usize)> = None;
    let mut subtree_end: Option<usize> = None;

    for (index, line) in lines.iter().enumerate() {
        let Some((hashes, id)) = node_heading_outside_code(line, &mut in_code_block) else {
            continue;
        };
        match parent {
            None if id == parent_local_id => parent = Some((index, hashes)),
            // The subtree ends at the first heading no deeper than the parent.
            Some((_, parent_hashes)) if hashes <= parent_hashes => {
                subtree_end = Some(index);
                break;
            }
            _ => {}
        }
    }

    let (_, _) = parent?;
    let insert_at = subtree_end.unwrap_or(lines.len());
    let mut out: Vec<String> = lines[..insert_at].iter().map(|line| (*line).to_string()).collect();
    while out.last().is_some_and(|line| line.trim().is_empty()) {
        out.pop();
    }
    out.push(String::new());
    out.extend(block.trim_end_matches('\n').lines().map(ToOwned::to_owned));
    if insert_at < lines.len() {
        out.push(String::new());
        out.extend(lines[insert_at..].iter().map(|line| (*line).to_string()));
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    Some(joined)
}
