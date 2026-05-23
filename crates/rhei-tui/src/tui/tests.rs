use super::handle_key_event;
use super::render::slot_lines;
use super::state::UiState;
use super::text::{sanitize_terminal_text, truncate_chars};
use super::{InputAction, SLOT_TRAFFIC_BUFFER};
use crate::event::{AgentStream, RunEvent};
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

#[test]
fn ctrl_c_requests_sigint_forwarding() {
    let mut state = UiState::new(1, 1);

    let action = handle_key_event(&mut state, KeyCode::Char('c'), KeyModifiers::CONTROL);

    assert!(matches!(action, InputAction::ForwardSigint));
    assert_eq!(
        state.journal.back().map(String::as_str),
        Some("(ctrl+c received — forwarding SIGINT)")
    );
}

#[test]
fn non_ctrl_c_input_is_ignored() {
    let mut state = UiState::new(1, 1);

    let action = handle_key_event(&mut state, KeyCode::Char('q'), KeyModifiers::NONE);

    assert!(matches!(action, InputAction::Continue));
    assert!(state.journal.is_empty());
}

#[test]
fn agent_output_is_added_to_slot_and_journal() {
    let mut state = UiState::new(1, 1);
    state.apply(&RunEvent::AgentOutput {
        slot: 0,
        task: "task-1".to_string(),
        stream: AgentStream::Stdout,
        line: "hello".to_string(),
        wall_clock: SystemTime::now(),
    });

    assert_eq!(state.slots[0].traffic.len(), 1);
    assert_eq!(state.slots[0].traffic[0].text, "hello");
    assert_eq!(state.journal.back().map(String::as_str), Some("· [slot 0 stdout] hello"));
}

#[test]
fn agent_output_retention_is_bounded() {
    let mut state = UiState::new(1, 1);
    for i in 0..(SLOT_TRAFFIC_BUFFER + 2) {
        state.apply(&RunEvent::AgentOutput {
            slot: 0,
            task: "task-1".to_string(),
            stream: AgentStream::Stderr,
            line: format!("line {i}"),
            wall_clock: SystemTime::now(),
        });
    }

    assert_eq!(state.slots[0].traffic.len(), SLOT_TRAFFIC_BUFFER);
    assert_eq!(state.slots[0].traffic[0].text, "line 2");
}

#[test]
fn unknown_slot_output_does_not_panic() {
    let mut state = UiState::new(1, 1);
    state.apply(&RunEvent::AgentOutput {
        slot: 9,
        task: "task-1".to_string(),
        stream: AgentStream::Stdout,
        line: "orphan".to_string(),
        wall_clock: SystemTime::now(),
    });

    assert!(state.slots[0].traffic.is_empty());
    assert_eq!(state.journal.back().map(String::as_str), Some("· [slot 9 stdout] orphan"));
}

#[test]
fn sanitizes_control_sequences_for_display() {
    assert_eq!(sanitize_terminal_text("\u{1b}[31mred\u{1b}[0m"), "red");
    assert_eq!(sanitize_terminal_text("a\u{7}b"), "ab");
}

#[test]
fn truncates_with_ellipsis() {
    assert_eq!(truncate_chars("abcdef", 4), "abc…");
    assert_eq!(truncate_chars("abc", 4), "abc");
}

#[test]
fn slot_lines_reserve_rows_for_later_slots() {
    let mut state = UiState::new(3, 3);
    for slot in 0..3 {
        state.apply(&RunEvent::SlotAssigned {
            slot,
            task: format!("task-{slot}"),
            from: "fetch".to_string(),
            to: "fetch".to_string(),
            agent: Some("codex".to_string()),
            template_context: None,
            log_path: PathBuf::from(format!("task-{slot}.log")),
            started_at: Instant::now(),
            wall_clock: SystemTime::now(),
        });
    }
    for i in 0..10 {
        state.apply(&RunEvent::AgentOutput {
            slot: 0,
            task: "task-0".to_string(),
            stream: AgentStream::Stdout,
            line: format!("line {i}"),
            wall_clock: SystemTime::now(),
        });
    }

    let snapshot = state.clone_snapshot();
    let lines = slot_lines(&snapshot, 100, 3);
    let rendered = lines
        .iter()
        .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
        .collect::<Vec<_>>();

    assert_eq!(rendered.len(), 3);
    assert!(rendered.iter().any(|line| line.contains("task-0")));
    assert!(rendered.iter().any(|line| line.contains("task-1")));
    assert!(rendered.iter().any(|line| line.contains("task-2")));
}
