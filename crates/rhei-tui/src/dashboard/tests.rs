use super::*;
use crate::event::{
    DimensionStatus, DimensionSummary, PricingStatus, RunSummary, TaskOutcome, UsageCoverage,
    UsageStatus, UsageSummary,
};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

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

fn dashboard_task_with_details(
    id: &str,
    title: &str,
    state: &str,
    parent: Option<&str>,
    depth: u8,
) -> DashboardTask {
    DashboardTask {
        id: id.to_string(),
        title: title.to_string(),
        kind: "Task".to_string(),
        parent: parent.map(str::to_string),
        depth,
        state: state.to_string(),
        assignee: Some("agent-a".to_string()),
        prior: vec!["0".to_string()],
        result_link: Some(format!("runtime/results/{id}.md")),
    }
}

fn fetch_snapshot_json(dashboard: &DashboardSink) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match fetch_snapshot_json_once(dashboard) {
            Ok(snapshot) => return snapshot,
            Err(message) if message.contains("invalid JSON") => panic!("{message}"),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(message) => panic!("{message}"),
        }
    }
}

fn fetch_snapshot_json_once(dashboard: &DashboardSink) -> Result<serde_json::Value, String> {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream =
        TcpStream::connect(addr).map_err(|err| format!("connect to dashboard: {err}"))?;
    stream
        .write_all(b"GET /snapshot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .map_err(|err| format!("write request: {err}"))?;
    let response =
        read_http_response(&mut stream).map_err(|err| format!("read response: {err}"))?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "response missing HTTP body separator".to_string())?;
    if !response.starts_with(b"HTTP/1.1 200 OK\r\n") {
        return Err("dashboard snapshot request did not return HTTP 200".to_string());
    }
    let body = &response[split + 4..];
    if body.is_empty() {
        return Err("dashboard snapshot response body was empty".to_string());
    }
    serde_json::from_slice(body).map_err(|err| format!("invalid JSON snapshot: {err}"))
}

fn dashboard_http_test_guard() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
fn plan_state_derivation_treats_running_root_as_active() {
    let tasks = vec![
        dashboard_task("1", "work", None, 1),
        dashboard_task("1.1", "review", Some("1"), 2),
        dashboard_task("2", "pending", None, 1),
    ];
    let active = HashSet::from(["1"]);

    assert_eq!(derive_plan_state_with_active_roots(&tasks, &active), "active");
}

#[test]
fn plan_state_derivation_ignores_running_child_for_plan_state() {
    let tasks =
        vec![dashboard_task("1", "draft", None, 1), dashboard_task("1.1", "review", Some("1"), 2)];
    let active = HashSet::from(["1.1"]);

    assert_eq!(derive_plan_state_with_active_roots(&tasks, &active), "draft");
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

/// `/snapshot` exposes the flattened plan data and runtime projections used
/// by all dashboard visualization and operational tabs. §FS-rhei-viz.4
#[test]
fn dashboard_snapshot_endpoint_exposes_plan_rows_and_runtime_projection() {
    let _guard = dashboard_http_test_guard();
    let loader: PlanLoader = Arc::new(|| {
        Some(PlanSnapshot {
            title: "Demo Plan".to_string(),
            tasks: vec![
                dashboard_task_with_details("1", "Root", "pending", None, 1),
                dashboard_task_with_details("1.1", "Child", "agent-review", Some("1"), 2),
                dashboard_task_with_details("2", "Deferred", "pending", None, 1),
            ],
        })
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 2, 3, Some(loader))
        .expect("start dashboard");

    dashboard.emit(RunEvent::PassStarted { pass: 1, ready: vec!["2".to_string()] });
    dashboard.emit(RunEvent::TasksDeferred { pass: 1, tasks: vec!["2".to_string()] });
    dashboard.emit(RunEvent::SlotAssigned {
        slot: 1,
        task: "1.1".to_string(),
        from: "agent-review".to_string(),
        to: "agent-review".to_string(),
        agent: Some("codex".to_string()),
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["plan_title"], "Demo Plan");
    assert_eq!(snapshot["plan_state"], "pending");
    assert_eq!(snapshot["tasks"].as_array().expect("tasks").len(), 3);

    let child = snapshot["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["id"] == "1.1")
        .expect("child task");
    assert_eq!(child["title"], "Child");
    assert_eq!(child["parent"], "1");
    assert_eq!(child["depth"], 2);
    assert_eq!(child["state"], "agent-review");
    assert_eq!(child["assignee"], "agent-a");
    assert_eq!(child["prior"][0], "0");
    assert_eq!(child["result_link"], "runtime/results/1.1.md");
    assert_eq!(child["in_slot"], 1);
    assert_eq!(child["deferred_this_pass"], false);

    let deferred = snapshot["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["id"] == "2")
        .expect("deferred task");
    assert_eq!(deferred["deferred_this_pass"], true);
}

#[test]
fn dashboard_snapshot_endpoint_marks_running_custom_root_active() {
    let _guard = dashboard_http_test_guard();
    let loader: PlanLoader = Arc::new(|| {
        Some(PlanSnapshot {
            title: "Demo Plan".to_string(),
            tasks: vec![
                dashboard_task_with_details("1", "Root", "work", None, 1),
                dashboard_task_with_details("1.1", "Child", "review", Some("1"), 2),
                dashboard_task_with_details("2", "Deferred", "pending", None, 1),
            ],
        })
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 2, Some(loader))
        .expect("start dashboard");

    dashboard.emit(RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: "work".to_string(),
        to: "work".to_string(),
        agent: None,
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["plan_state"], "active");
}

#[test]
fn dashboard_snapshot_endpoint_does_not_mark_released_slot_active() {
    let _guard = dashboard_http_test_guard();
    let loader: PlanLoader = Arc::new(|| {
        Some(PlanSnapshot {
            title: "Demo Plan".to_string(),
            tasks: vec![
                dashboard_task_with_details("1", "Root", "completed", None, 1),
                dashboard_task_with_details("2", "Next", "pending", None, 1),
            ],
        })
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 2, Some(loader))
        .expect("start dashboard");

    dashboard.emit(RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: "work".to_string(),
        to: "work".to_string(),
        agent: None,
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    dashboard.emit(RunEvent::SlotReleased {
        slot: 0,
        task: "1".to_string(),
        from: "work".to_string(),
        to: "completed".to_string(),
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        outcome: TaskOutcome::Completed,
        finished_at: Instant::now(),
        wall_clock: SystemTime::now(),
        exit_code: Some(0),
        duration_ms: 10,
    });

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["plan_state"], "pending");
    let task = snapshot["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["id"] == "1")
        .expect("task 1");
    assert!(task["in_slot"].is_null());
}

fn dashboard_usage(
    invocation_id: &str,
    coverage: UsageCoverage,
    pricing_status: PricingStatus,
    cost_micro: Option<u64>,
    priced_cost_micro: Option<u64>,
) -> UsageSummary {
    let measured = DimensionSummary {
        value: Some(1),
        status: DimensionStatus::Measured,
        missing_count: 0,
        measured_count: 1,
    };
    UsageSummary {
        invocation_id: invocation_id.to_string(),
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
        input_total: measured.clone(),
        input_cached_read: measured.clone(),
        input_cache_write: measured.clone(),
        output_total: measured.clone(),
        output_cached_read: measured.clone(),
        output_cache_write: measured,
        cost_micro,
        priced_cost_micro,
        currency: Some("USD".to_string()),
        coverage,
        status: UsageStatus::Measured,
        pricing_status,
    }
}

#[test]
fn dashboard_mixed_priced_and_unpriced_rollup_is_partial() {
    let mut state = empty_state();
    state.apply(&RunEvent::UsageReported {
        slot: None,
        task: "1".to_string(),
        invocation_id: "priced".to_string(),
        usage: dashboard_usage(
            "priced",
            UsageCoverage::Complete,
            PricingStatus::Priced,
            Some(100),
            Some(100),
        ),
    });
    state.apply(&RunEvent::UsageReported {
        slot: None,
        task: "2".to_string(),
        invocation_id: "unpriced".to_string(),
        usage: dashboard_usage(
            "unpriced",
            UsageCoverage::Unpriced,
            PricingStatus::Unpriced,
            None,
            None,
        ),
    });

    let accounting = state.accounting.expect("accounting");
    assert_eq!(accounting.coverage, UsageCoverage::Partial);
    assert_eq!(accounting.pricing_status, PricingStatus::PartialPrice);
    assert_eq!(accounting.cost_micro, None);
    assert_eq!(accounting.priced_cost_micro, Some(100));
}

/// Dashboard-enabled runs leave an inspectable static artifact behind after
/// the loopback server exits. §FS-rhei-viz.1
#[test]
fn frozen_dashboard_writes_self_contained_final_artifact() {
    let _guard = dashboard_http_test_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let loader: PlanLoader = Arc::new(|| {
        Some(PlanSnapshot {
            title: "Frozen Plan".to_string(),
            tasks: vec![dashboard_task("1", "completed", None, 1)],
        })
    });
    let dashboard = DashboardSink::start_with_plan(temp.path().to_path_buf(), 1, 1, Some(loader))
        .expect("start dashboard");
    dashboard.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned: 0,
            programs_spawned: 1,
            terminal_tasks: 1,
            total_tasks: 1,
            accounting: None,
        },
    });

    let path = dashboard.write_frozen_dashboard().expect("write frozen dashboard");
    assert_eq!(path, temp.path().join("runtime/dashboard.html"));
    let html = fs::read_to_string(path).expect("frozen html");
    assert!(html.contains("const FINAL_SNAPSHOT = "));
    assert!(html.contains("\"plan_title\":\"Frozen Plan\""));
    assert!(html.contains("render(FINAL_SNAPSHOT);"));
    assert!(html.contains("Snapshot is frozen at the final state"));
    assert!(!html.contains("setInterval(tick, 1000);"));
}

/// Temporary plan reload failures keep serving the last good plan snapshot.
/// §FS-rhei-viz.1
#[test]
fn dashboard_snapshot_endpoint_keeps_last_good_plan_snapshot() {
    let _guard = dashboard_http_test_guard();
    let calls = Arc::new(AtomicUsize::new(0));
    let loader_calls = Arc::clone(&calls);
    let loader: PlanLoader = Arc::new(move || {
        if loader_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Some(PlanSnapshot {
                title: "Cached Plan".to_string(),
                tasks: vec![dashboard_task("1", "draft", None, 1)],
            })
        } else {
            None
        }
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, Some(loader))
        .expect("start dashboard");

    let first = fetch_snapshot_json(&dashboard);
    let second = fetch_snapshot_json(&dashboard);

    assert_eq!(first["plan_title"], "Cached Plan");
    assert_eq!(second["plan_title"], "Cached Plan");
    assert_eq!(second["plan_state"], "draft");
    assert_eq!(second["tasks"].as_array().expect("tasks").len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// Visualization tabs lead the dashboard and the HTML stays self-contained.
/// §FS-rhei-viz.1 §FS-rhei-viz
#[test]
fn dashboard_html_includes_visualization_tabs_and_stays_self_contained() {
    let expected_tabs = [
        r#"data-view="gantt""#,
        r#"data-view="cube""#,
        r#"data-view="sankey""#,
        r#"data-view="tasks""#,
        r#"data-view="slots""#,
        r#"data-view="journal""#,
        r#"data-view="links""#,
    ];
    let mut cursor = 0;
    for tab in expected_tabs {
        let found = DASHBOARD_HTML[cursor..].find(tab).expect("tab in order");
        cursor += found + tab.len();
    }
    assert!(DASHBOARD_HTML.contains(r#"<button class="tab active" data-view="gantt">"#));
    assert!(DASHBOARD_HTML.contains("function descendantsByRoot"));
    assert!(DASHBOARD_HTML.contains("function cubeColumnSlots"));
    assert!(DASHBOARD_HTML.contains("function isRunnableTask"));
    assert!(DASHBOARD_HTML.contains("child state ·"));
    assert!(DASHBOARD_HTML.contains("FALLBACK_STATE_COLORS"));
    assert!(DASHBOARD_HTML.contains("rowCount ? rowCount + 1"));
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
    assert!(!DASHBOARD_HTML.contains("@import"));
}
