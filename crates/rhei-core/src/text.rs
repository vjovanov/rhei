use crate::ast::TaskId;

pub(crate) fn parse_task_id(input: &str) -> TaskId {
    input
        .parse::<u32>()
        .ok()
        .map(TaskId::Number)
        .unwrap_or_else(|| TaskId::Named(input.to_string()))
}

pub(crate) fn unescape_simple(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(chars.next().unwrap_or('\\'));
        } else {
            out.push(c);
        }
    }
    out
}
