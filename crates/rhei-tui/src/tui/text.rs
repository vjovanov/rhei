use crate::event::AgentStream;

pub(super) fn stream_label(stream: AgentStream) -> &'static str {
    match stream {
        AgentStream::Stdout => "stdout",
        AgentStream::Stderr => "stderr",
    }
}

pub(super) fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        if let Some(ch) = chars.next() {
            out.push(ch);
        } else {
            return out;
        }
    }
    if chars.next().is_some() {
        if max_chars > 1 {
            out.pop();
        }
        out.push('…');
    }
    out
}

pub(super) fn sanitize_terminal_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if matches!(chars.peek(), Some('[')) {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        if ch.is_control() && ch != '\t' {
            continue;
        }
        out.push(ch);
    }
    out
}
