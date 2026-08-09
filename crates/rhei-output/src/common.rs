use rhei_core::ast::{Task, TaskId, TaskIdSegment};

/// One rhei of a merged Panta project, as the text renderers present it.
///
/// A merged project flattens every rhei's tickets into one list under one set
/// of headings, which reads as a single undifferentiated plan. Grouping puts
/// each ticket back under the workstream that owns it.
// §AR-rhei-panta.3: a ticket's leading id segment names its owning rhei.
pub(crate) struct RheiGroup {
    /// Rhei id — the leading segment of every ticket id it owns.
    pub id: String,
    /// The rhei's own title, without the `Rhei <id>: ` merge prefix.
    pub title: String,
}

impl RheiGroup {
    /// True when `task` is a top-level ticket of this rhei.
    pub fn owns(&self, task: &Task) -> bool {
        matches!(task.id.segments.first(), Some(TaskIdSegment::Named(name)) if *name == self.id)
    }

    /// A merged section title (`Rhei billing / Overview`) as authored
    /// (`Overview`); the rhei is already named by the enclosing group.
    pub fn section_title<'a>(&self, merged: &'a str) -> &'a str {
        merged.strip_prefix(&format!("Rhei {} / ", self.id)).unwrap_or(merged)
    }
}

/// The rheis of a merged Panta project, in merge order.
///
/// Empty for any plan that is not a merged project — a single-file plan or a
/// Directory Workspace loaded on its own — which renders in its authored shape.
pub(crate) fn rhei_groups(rhei: &rhei_core::ast::Rhei) -> Vec<RheiGroup> {
    rhei.content_sections
        .iter()
        .filter_map(|section| {
            let id = section.rhei.as_ref()?;
            // The merge emits exactly one `Rhei <id>: <title>` header per rhei,
            // ahead of that rhei's own sections; the rest are its content.
            let title = section.title.strip_prefix(&format!("Rhei {id}: "))?;
            Some(RheiGroup { id: id.clone(), title: title.to_string() })
        })
        .collect()
}

pub(crate) fn title_case_kind(kind: &str) -> String {
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

pub(crate) fn fmt_prior_list(ids: &[TaskId]) -> String {
    ids.iter().map(|id| format!("Task {}", id)).collect::<Vec<String>>().join(", ")
}
