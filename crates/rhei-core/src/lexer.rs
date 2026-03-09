//! Line-oriented tokenizer producing a stream of tokens from markdown input.
//!
//! This initial implementation focuses on the primary structures:
//! - Saga header
//! - Tasks section
//! - Task and Subtask headers
//! - Metadata fields: State and Prior
//! - Text content (non-empty lines that are not matched by the above)
//!
//! Edge cases like fenced code blocks, escapes, and nested markdown will be
//! addressed in Task 2.3.

use crate::ast::TaskId;
use crate::tokens::Token;
use regex::Regex;

/// Streaming tokenizer over markdown input.
pub struct Tokenizer<'a> {
    lines: std::str::Lines<'a>,

    re_saga: Regex,
    re_tasks: Regex,
    re_task_header: Regex,
    re_subtask_header: Regex,
    re_prior_task_id: Regex,
    re_state: Regex,

    in_code_block: bool,
}

impl<'a> Tokenizer<'a> {
    /// Construct a new tokenizer over the provided input.
    pub fn new(input: &'a str) -> Self {
        // Compile patterns once per tokenizer instantiation.
        let re_saga = Regex::new(r#"^#\s+Saga:\s+.*$"#).unwrap();
        let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
        let re_task_header =
            Regex::new(r#"^###\s+Task\s+([A-Za-z][A-Za-z0-9_-]*|\d+):\s+.*$"#).unwrap();
        let re_subtask_header = Regex::new(r#"^####\s+Subtask\s+(\d+)\.(\d+):\s+.*$"#).unwrap();

        // For "**Prior:** Task 1, Task 2" or named ids
        let re_prior_task_id = Regex::new(r#"Task\s+([A-Za-z][A-Za-z0-9_-]*|\d+)"#).unwrap();

        // For "**State:** value"
        let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();

        Self {
            lines: input.lines(),
            re_saga,
            re_tasks,
            re_task_header,
            re_subtask_header,
            re_prior_task_id,
            re_state,
            in_code_block: false,
        }
    }

    /// Unescape simple backslash escapes used in metadata values.
    /// For now we support:
    /// - "\ " -> " "
    /// - "\\" -> "\"
    fn unescape_simple(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                } else {
                    // Trailing backslash, keep as-is
                    out.push('\\');
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Detect start/end of a fenced code block (``` optional language).
    fn is_fence(line: &str) -> bool {
        // Allow optional leading spaces; fence starts with at least three backticks
        let trimmed = line.trim_start();
        trimmed.starts_with("```")
    }
}

/// Convenience constructor to obtain a streaming tokenizer.
pub fn tokenize(input: &str) -> Tokenizer<'_> {
    Tokenizer::new(input)
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        for raw in self.lines.by_ref() {
            let line = raw.trim();

            // Toggle code block fences before any matching
            if Self::is_fence(raw) {
                self.in_code_block = !self.in_code_block;
                // Treat fence line as content
                return Some(Token::TextContent);
            }

            // Skip empty lines entirely (no token emitted)
            if line.is_empty() {
                continue;
            }

            // When inside code blocks, do not attempt structural matches.
            if self.in_code_block {
                return Some(Token::TextContent);
            }

            if self.re_saga.is_match(line) {
                return Some(Token::SagaHeader);
            }

            if self.re_tasks.is_match(line) {
                return Some(Token::TasksSection);
            }

            if let Some(caps) = self.re_task_header.captures(line) {
                if let Some(m) = caps.get(1) {
                    let s = m.as_str();
                    let id = s
                        .parse::<u32>()
                        .ok()
                        .map(TaskId::Number)
                        .unwrap_or_else(|| TaskId::Named(s.to_string()));
                    return Some(Token::TaskHeader { id });
                }
            }

            if let Some(caps) = self.re_subtask_header.captures(line) {
                let tn = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
                let sn = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
                if let (Some(task_number), Some(subtask_number)) = (tn, sn) {
                    return Some(Token::SubtaskHeader { task_number, subtask_number });
                }
            }

            // Metadata: State (with unescaping)
            if let Some(caps) = self.re_state.captures(line) {
                let state_raw = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
                let state = Self::unescape_simple(state_raw);
                return Some(Token::MetadataState { state });
            }

            // Metadata: Prior
            if line.starts_with("**Prior:**") {
                let ids = self
                    .re_prior_task_id
                    .captures_iter(line)
                    .filter_map(|c| c.get(1))
                    .map(|m| {
                        let s = m.as_str();
                        s.parse::<u32>()
                            .ok()
                            .map(TaskId::Number)
                            .unwrap_or_else(|| TaskId::Named(s.to_string()))
                    })
                    .collect::<Vec<TaskId>>();
                return Some(Token::MetadataPrior { task_ids: ids });
            }

            // Fallback for any other non-empty content.
            return Some(Token::TextContent);
        }

        None
    }
}
