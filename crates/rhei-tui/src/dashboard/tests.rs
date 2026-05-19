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

fn dashboard_task(id: &str, state: &str, parent: Option<&str>, depth: u8) -> DashboardTask {
    DashboardTask {
        id: id.to_string(),
        title: format!("Task {id}"),
        kind: "Task".to_string(),
        parent: parent.map(str::to_string),
        depth,
        state: state.to_string(),
        assignee: None,
        prior: Vec::new(),
        result_link: None,
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

#[test]
fn derives_plan_state_from_root_tasks() {
    assert_eq!(derive_plan_state(&[dashboard_task("1", "draft", None, 1)]), "draft");
    assert_eq!(
        derive_plan_state(&[
            dashboard_task("1", "completed", None, 1),
            dashboard_task("2", "completed", None, 1),
        ]),
        "completed"
    );
    assert_eq!(
        derive_plan_state(&[
            dashboard_task("1", "completed", None, 1),
            dashboard_task("2", "cancelled", None, 1),
        ]),
        "archived"
    );
    assert_eq!(
        derive_plan_state(&[
            dashboard_task("1", "pending", None, 1),
            dashboard_task("2", "agent-review-fix", None, 1),
        ]),
        "active"
    );
    assert_eq!(
        derive_plan_state(&[
            dashboard_task("1", "pending", None, 1),
            dashboard_task("2", "blocked", None, 1),
        ]),
        "pending"
    );
}

#[test]
fn plan_state_derivation_ignores_child_task_states() {
    let tasks = vec![
        dashboard_task("1", "draft", None, 1),
        dashboard_task("1.1", "agent-review", Some("1"), 2),
        dashboard_task("1.2", "failed", Some("1"), 2),
    ];

    assert_eq!(derive_plan_state(&tasks), "draft");
}

#[test]
fn snapshot_payload_exposes_plan_state() {
    let state = empty_state();
    let payload = SnapshotPayload {
        state: &state,
        plan_title: Some("Demo".to_string()),
        plan_state: Some("active".to_string()),
        tasks: Vec::new(),
        auto_links: Vec::new(),
    };

    let value = serde_json::to_value(&payload).expect("serialize snapshot payload");
    assert_eq!(value["plan_state"], "active");
}

#[test]
fn dashboard_html_includes_visualization_tabs_and_stays_self_contained() {
    assert!(DASHBOARD_HTML.contains(r#"data-view="gantt""#));
    assert!(DASHBOARD_HTML.contains(r#"data-view="cube""#));
    assert!(DASHBOARD_HTML.contains(r#"data-view="sankey""#));
    assert!(DASHBOARD_HTML.contains(r#"<button class="tab active" data-view="gantt">"#));
    assert!(DASHBOARD_HTML.contains("function descendantsByRoot"));
    assert!(DASHBOARD_HTML.contains("function cubeColumnSlots"));
    assert!(DASHBOARD_HTML.contains("Dense task-by-descendant-state heatmap"));
    assert!(DASHBOARD_HTML.contains("Descendant-state flow by top-level task"));
    assert!(DASHBOARD_HTML.contains("overflow-x: auto"));
    assert!(DASHBOARD_HTML.contains("white-space: nowrap"));
    assert!(DASHBOARD_HTML.contains("String(task.id).slice(prefix.length)"));
    assert!(DASHBOARD_HTML.contains("<title>${escapeHtml(slot)}</title>"));
    assert!(DASHBOARD_HTML
        .contains("new Map(descendants.map(child => [descendantSlot(root, child), child]))"));
    assert!(DASHBOARD_HTML.contains("<title>${title}</title>"));
    assert!(!DASHBOARD_HTML.contains("<script src="));
    assert!(!DASHBOARD_HTML.contains("<link rel=\"stylesheet\""));
}
