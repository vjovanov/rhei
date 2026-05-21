use crate::ast::{TaskId, TaskIdSegment};

/// Parse a single id segment (numeric or named).
pub(crate) fn parse_task_id_segment(input: &str) -> TaskIdSegment {
    if !input.is_empty() && input.bytes().all(|byte| byte.is_ascii_digit()) {
        let n = input.parse::<u32>().expect("numeric task id segment must fit in u32");
        return TaskIdSegment::Number(n);
    }
    TaskIdSegment::Named(input.to_string())
}

/// Parse a dotted task id (`1`, `1.2`, `api.cache.fix`) into a `TaskId`.
///
/// Returns `None` if the input is empty. Empty segments (e.g. `"1..2"`) are
/// rejected as malformed.
pub(crate) fn parse_task_id(input: &str) -> Option<TaskId> {
    if input.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    for part in input.split('.') {
        if part.is_empty() {
            return None;
        }
        if part.bytes().all(|byte| byte.is_ascii_digit())
            && ((part.len() > 1 && part.starts_with('0')) || part.parse::<u32>().is_err())
        {
            return None;
        }
        segments.push(parse_task_id_segment(part));
    }
    Some(TaskId::from_segments(segments))
}
