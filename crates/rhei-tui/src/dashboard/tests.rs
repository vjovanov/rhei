use super::*;
use std::time::{Instant, SystemTime};

fn empty_state() -> DashboardState {
    DashboardState::new(PathBuf::from("/tmp/ws"), 1, 1)
}

fn assigned(from: &str, to: &str) -> RunEvent {
    RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        agent: None,
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    }
}

/// Per `RunEvent::SlotAssigned`'s contract, `from == to` means the
/// engine started a worker in an autonomous state — not a transition.
/// `slot.transition` must stay `None` so renderers don't paint a phantom
/// state→state arrow.
#[test]
fn same_state_assignment_records_no_transition() {
    let mut state = empty_state();
    state.apply(&assigned("fetch", "fetch"));

    assert!(state.slots[0].active);
    assert_eq!(state.slots[0].state.as_deref(), Some("fetch"));
    assert!(
        state.slots[0].transition.is_none(),
        "from == to must not produce a transition; got {:?}",
        state.slots[0].transition
    );
    let last = state.recent.last().expect("recent line");
    assert!(
        last.text.contains("started in fetch"),
        "expected 'started in fetch'; got {:?}",
        last.text
    );
    assert!(
        !last.text.contains("fetch->fetch") && !last.text.contains("fetch→fetch"),
        "must not render a same-state arrow; got {:?}",
        last.text
    );
}

/// A real cross-state assignment must record both the `transition`
/// string and a `from->to` recent line.
#[test]
fn cross_state_assignment_records_arrow_transition() {
    let mut state = empty_state();
    state.apply(&assigned("draft", "pending"));

    assert_eq!(state.slots[0].transition.as_deref(), Some("draft->pending"));
    let last = state.recent.last().expect("recent line");
    assert!(
        last.text.contains("draft->pending"),
        "expected 'draft->pending' in recent; got {:?}",
        last.text
    );
}

#[test]
fn url_path_encodes_unsafe_bytes_and_preserves_slashes() {
    // Slashes, `:`, and unreserved chars stay verbatim; spaces and `#`
    // get percent-encoded; non-ASCII bytes are encoded byte-by-byte.
    assert_eq!(encode_url_path("/Users/me/project"), "/Users/me/project");
    assert_eq!(encode_url_path("/path with spaces/x"), "/path%20with%20spaces/x");
    assert_eq!(encode_url_path("/has#hash?and"), "/has%23hash%3Fand");
    // Two UTF-8 bytes for `é`.
    assert_eq!(encode_url_path("/caf\u{00e9}"), "/caf%C3%A9");
}
