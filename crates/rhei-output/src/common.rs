use rhei_core::ast::TaskId;

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
