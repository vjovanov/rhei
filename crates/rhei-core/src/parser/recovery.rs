use crate::ast::Rhei;

use super::{parse, ParseError};

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
    let whole_task_markers =
        ["missing mandatory **State:**", "malformed node heading", "expected '**State:** <value>'"];
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
