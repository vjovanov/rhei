use crate::ast::{Task, TaskId};

use super::{ParseError, Result};

pub(super) struct NodeBuilder {
    pub(super) id: TaskId,
    pub(super) kind: String,
    pub(super) title: String,
    pub(super) level: u8,
    pub(super) state: Option<String>,
    pub(super) prior: Vec<TaskId>,
    pub(super) assignee: Option<String>,
    pub(super) content: String,
    pub(super) children: Vec<Task>,
    /// Once non-metadata content appears, further metadata fields become
    /// errors.
    pub(super) metadata_closed: bool,
    /// Line number of the heading (for error reporting).
    pub(super) heading_line: usize,
}

fn finalize_builder(b: NodeBuilder) -> Result<Task> {
    let state = b.state.ok_or_else(|| {
        ParseError::new(
            format!(
                "{} {} is missing mandatory **State:** metadata",
                title_case_kind(&b.kind),
                b.id
            ),
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

pub(super) fn title_case_kind(kind: &str) -> String {
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
pub(super) fn unwind_to_level(
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
