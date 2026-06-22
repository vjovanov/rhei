use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyModifiers};
use rhei_viz_model::{Machine, MachineState, TaskRow, Transition, VizModel};

use super::input::{handle_key_event, InputAction};
use super::state::{CostGroup, FlowFocus, UiState, UsageRecord, View};
use super::text::{sanitize_terminal_text, truncate_chars};
use crate::dashboard::InterveneSink;
use crate::event::{
    AgentStream, DimensionStatus, DimensionSummary, MessageLevel, PricingStatus, RunEvent, Slot,
    TaskOutcome, UsageCoverage, UsageStatus, UsageSummary,
};

fn machine_state(name: &str, gating: bool, transitions: Vec<&str>) -> MachineState {
    MachineState {
        name: name.to_string(),
        description: None,
        instructions: None,
        visits: None,
        initial: name == "draft",
        terminal: matches!(name, "completed" | "done" | "cancelled"),
        gating,
        transitions: transitions
            .into_iter()
            .map(|to| Transition { to: to.to_string(), condition: None, wildcard: false })
            .collect(),
        inputs: vec![],
        outputs: vec![],
        template_context: Default::default(),
        template_contexts: vec![],
    }
}

fn demo_model() -> VizModel {
    VizModel {
        plan_title: Some("Demo".into()),
        plan_state: Some("active".into()),
        about: None,
        tasks: vec![
            TaskRow {
                id: "1".into(),
                title: "Alpha".into(),
                parent: None,
                depth: 0,
                state: "in-progress".into(),
                visit_count: None,
                prior: vec![],
            },
            TaskRow {
                id: "2".into(),
                title: "Beta".into(),
                parent: None,
                depth: 0,
                state: "human-review".into(),
                visit_count: None,
                prior: vec!["1".into()],
            },
        ],
        machine: Machine {
            name: "rhei".into(),
            states: vec![
                machine_state("draft", false, vec!["in-progress"]),
                machine_state("in-progress", false, vec!["human-review"]),
                machine_state("human-review", true, vec!["completed", "in-progress"]),
                machine_state("completed", false, vec![]),
            ],
        },
    }
}

fn state_with_plan() -> UiState {
    let mut state = UiState::with_context(PathBuf::from("/ws"), 2, 2, None, None, None);
    state.plan = demo_model();
    state.refresh_plan();
    state
}

fn press(state: &mut UiState, code: KeyCode) -> InputAction {
    handle_key_event(state, code, KeyModifiers::NONE)
}

#[test]
fn ctrl_c_requests_sigint_forwarding() {
    let mut state = state_with_plan();
    let action = handle_key_event(&mut state, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(action, InputAction::ForwardSigint));
}

#[test]
fn quit_only_after_finished() {
    let mut state = state_with_plan();
    assert!(matches!(press(&mut state, KeyCode::Char('q')), InputAction::Continue));
    state.finished = true;
    assert!(matches!(press(&mut state, KeyCode::Char('q')), InputAction::Quit));
}

#[test]
fn number_keys_switch_views() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('2'));
    assert!(state.view == View::Machine);
    press(&mut state, KeyCode::Char('5'));
    assert!(state.view == View::Tasks);
    press(&mut state, KeyCode::Char('1'));
    assert!(state.view == View::Flow);
}

#[test]
fn auto_selects_first_active_task() {
    let state = state_with_plan();
    // Task 1 is `in-progress` → state-derived active; selected on load.
    assert_eq!(state.selected.as_deref(), Some("1"));
}

#[test]
fn outline_movement_changes_selection() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('j'));
    assert_eq!(state.selected.as_deref(), Some("2"));
    press(&mut state, KeyCode::Char('k'));
    assert_eq!(state.selected.as_deref(), Some("1"));
}

#[test]
fn tab_toggles_flow_focus() {
    let mut state = state_with_plan();
    assert!(matches!(state.flow_focus, FlowFocus::Outline));
    press(&mut state, KeyCode::Tab);
    assert!(matches!(state.flow_focus, FlowFocus::Inspector));
}

#[test]
fn filter_narrows_visible_tasks() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('/'));
    press(&mut state, KeyCode::Char('B'));
    press(&mut state, KeyCode::Char('e'));
    press(&mut state, KeyCode::Enter);
    let visible = state.visible_task_indices();
    assert_eq!(visible.len(), 1);
    assert_eq!(state.plan.tasks[visible[0]].id, "2");
}

#[test]
fn filter_narrows_machine_states_and_keeps_focus_visible() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('2'));
    press(&mut state, KeyCode::Char('/'));
    for ch in "human".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);

    let states = state
        .machine_view_order()
        .iter()
        .map(|i| state.plan.machine.states[*i].name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(states, vec!["human-review"]);
    assert_eq!(state.plan.machine.states[state.machine_focus].name, "human-review");

    press(&mut state, KeyCode::Char('j'));
    assert_eq!(state.plan.machine.states[state.machine_focus].name, "human-review");
}

#[test]
fn filter_narrows_journal_lines() {
    let mut state = state_with_plan();
    state.push_journal(MessageLevel::Info, "alpha transition".into());
    state.push_journal(MessageLevel::Warn, "beta warning".into());
    press(&mut state, KeyCode::Char('4'));
    press(&mut state, KeyCode::Char('/'));
    for ch in "beta".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);

    let filtered = state.filtered_journal().iter().map(|e| e.text.as_str()).collect::<Vec<_>>();
    assert_eq!(filtered, vec!["beta warning"]);
}

#[test]
fn tasks_state_filter_cycles_from_selected_state() {
    let mut state = state_with_plan();
    press(&mut state, KeyCode::Char('5'));

    assert_eq!(state.tasks_state_filter_label(), "all");
    assert_eq!(
        state
            .tasks_view_order()
            .iter()
            .map(|i| state.plan.tasks[*i].id.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "2"]
    );

    press(&mut state, KeyCode::Char('f'));
    assert_eq!(state.tasks_state_filter_label(), "in-progress");
    assert_eq!(
        state
            .tasks_view_order()
            .iter()
            .map(|i| state.plan.tasks[*i].id.as_str())
            .collect::<Vec<_>>(),
        vec!["1"]
    );

    press(&mut state, KeyCode::Char('f'));
    assert_eq!(state.tasks_state_filter_label(), "human-review");
    assert_eq!(
        state
            .tasks_view_order()
            .iter()
            .map(|i| state.plan.tasks[*i].id.as_str())
            .collect::<Vec<_>>(),
        vec!["2"]
    );

    press(&mut state, KeyCode::Char('f'));
    assert_eq!(state.tasks_state_filter_label(), "all");
}

#[test]
fn agent_output_is_recorded_on_slot() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("codex".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    state.apply(&RunEvent::AgentOutput {
        slot: 0,
        task: "1".into(),
        stream: AgentStream::Stdout,
        line: "hello".into(),
        wall_clock: SystemTime::now(),
    });
    assert!(state.is_live("1"));
    assert_eq!(state.slots[0].traffic.back().map(|t| t.text.as_str()), Some("hello"));
}

#[test]
fn slot_release_clears_live_marker() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: None,
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    assert!(state.is_live("1"));
    state.apply(&RunEvent::SlotReleased {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "human-review".into(),
        log_path: PathBuf::from("1.log"),
        outcome: TaskOutcome::Completed,
        finished_at: Instant::now(),
        wall_clock: SystemTime::now(),
        exit_code: Some(0),
        duration_ms: 1200,
    });
    assert!(!state.is_live("1"));
}

#[test]
fn journal_filter_keeps_only_warnings() {
    let mut state = state_with_plan();
    state.push_journal(MessageLevel::Info, "info".into());
    state.push_journal(MessageLevel::Warn, "warn".into());
    state.journal_filter = super::state::JournalFilter::Warnings;
    let filtered: Vec<_> = state.filtered_journal().iter().map(|e| e.text.clone()).collect();
    assert_eq!(filtered, vec!["warn".to_string()]);
}

#[test]
fn dashboard_link_is_pinned_in_header() {
    let mut state = state_with_plan();
    state.apply(&RunEvent::RunLink {
        label: "Dashboard".into(),
        url: "http://127.0.0.1:54321".into(),
    });
    assert_eq!(state.dashboard_url.as_deref(), Some("http://127.0.0.1:54321"));
}

#[test]
fn gate_choices_list_explicit_transitions() {
    let mut state = state_with_plan();
    state.select_task("2"); // human-review, gating
    let choices = super::input::gate_choices(&state);
    assert_eq!(choices.len(), 2);
    assert!(choices.iter().any(|(_, to)| to == "completed"));
}

struct ReachableSink {
    delivered: Mutex<Vec<String>>,
}

impl InterveneSink for ReachableSink {
    fn deliver(
        &self,
        _task_id: Option<&str>,
        _slot: Option<Slot>,
        message: &str,
    ) -> Result<(), String> {
        self.delivered.lock().unwrap().push(message.to_string());
        Ok(())
    }
    fn reachable(&self, _task_id: &str, _slot: Option<Slot>) -> bool {
        true
    }
}

#[test]
fn intervene_composer_delivers_message() {
    let sink = Arc::new(ReachableSink { delivered: Mutex::new(Vec::new()) });
    let mut state = state_with_plan();
    state.intervene = Some(sink.clone());
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("claude-code".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    state.select_task("1");
    press(&mut state, KeyCode::Char('m'));
    assert!(state.composer.is_some());
    for ch in "ping".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Enter);
    assert!(state.composer.is_none());
    assert_eq!(sink.delivered.lock().unwrap().as_slice(), ["ping".to_string()]);
}

#[test]
fn intervene_unreachable_when_not_live() {
    let mut state = state_with_plan();
    state.select_task("1");
    press(&mut state, KeyCode::Char('m'));
    // No running slot for task 1 → composer must not open.
    assert!(state.composer.is_none());
}

fn measured(value: u64) -> DimensionSummary {
    DimensionSummary {
        value: Some(value),
        status: DimensionStatus::Measured,
        missing_count: 0,
        measured_count: 1,
    }
}

fn demo_usage() -> UsageSummary {
    UsageSummary {
        invocation_id: "inv-1".into(),
        state: "in-progress".into(),
        agent: "claude-code".into(),
        provider: Some("anthropic".into()),
        model: Some("claude".into()),
        total: measured(1200),
        input_total: measured(1000),
        input_cached_read: measured(400),
        input_cache_write: measured(0),
        output_total: measured(200),
        output_cached_read: measured(0),
        output_cache_write: measured(0),
        cost_micro: Some(2_500_000),
        priced_cost_micro: Some(2_500_000),
        currency: Some("USD".into()),
        coverage: UsageCoverage::Complete,
        status: UsageStatus::Measured,
        pricing_status: PricingStatus::Priced,
    }
}

#[test]
fn cost_state_grouping_uses_invocation_state() {
    let mut state = state_with_plan();
    let mut usage = demo_usage();
    usage.cost_micro = Some(100);
    usage.priced_cost_micro = Some(100);
    state.invocations.push(UsageRecord { task: "1".into(), usage });

    let mut usage = demo_usage();
    usage.invocation_id = "inv-2".into();
    usage.state = "human-review".into();
    usage.cost_micro = Some(200);
    usage.priced_cost_micro = Some(200);
    state.invocations.push(UsageRecord { task: "2".into(), usage });
    state.cost_group = CostGroup::State;

    let rows = super::views::cost_rows(&state);

    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        vec!["human-review", "in-progress",]
    );
    assert_eq!(
        rows.iter().map(|row| row.1.cost_micro).collect::<Vec<_>>(),
        vec![Some(200), Some(100),]
    );
}

#[test]
fn task_readiness_uses_machine_terminal_priors() {
    let mut state = state_with_plan();
    state.plan.tasks = vec![
        TaskRow {
            id: "1".into(),
            title: "Done".into(),
            parent: None,
            depth: 0,
            state: "done".into(),
            visit_count: None,
            prior: vec![],
        },
        TaskRow {
            id: "2".into(),
            title: "Dependent".into(),
            parent: None,
            depth: 0,
            state: "draft".into(),
            visit_count: None,
            prior: vec!["1".into()],
        },
    ];
    state.plan.machine.states.push(machine_state("done", false, vec![]));
    let dependent = &state.plan.tasks[1];

    assert_eq!(state.task_ready(dependent), "ready");
    assert!(state.unresolved_priors(dependent).is_empty());
}

#[test]
fn cancelled_terminal_priors_do_not_unblock_tasks() {
    let mut state = state_with_plan();
    state.plan.tasks = vec![
        TaskRow {
            id: "1".into(),
            title: "Cancelled".into(),
            parent: None,
            depth: 0,
            state: "cancelled".into(),
            visit_count: None,
            prior: vec![],
        },
        TaskRow {
            id: "2".into(),
            title: "Dependent".into(),
            parent: None,
            depth: 0,
            state: "draft".into(),
            visit_count: None,
            prior: vec!["1".into()],
        },
    ];
    state.plan.machine.states.push(machine_state("cancelled", false, vec![]));
    let dependent = &state.plan.tasks[1];

    assert_eq!(state.task_ready(dependent), "blocked");
    assert_eq!(state.unresolved_priors(dependent), vec!["1".to_string()]);
}

#[test]
fn renders_every_view_and_overlay_without_panic() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut state = state_with_plan();
    state.apply(&RunEvent::SlotAssigned {
        slot: 0,
        task: "1".into(),
        from: "in-progress".into(),
        to: "in-progress".into(),
        agent: Some("claude-code".into()),
        template_context: None,
        log_path: PathBuf::from("1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    state.apply(&RunEvent::AgentOutput {
        slot: 0,
        task: "1".into(),
        stream: AgentStream::Stdout,
        line: "working…".into(),
        wall_clock: SystemTime::now(),
    });
    state.apply(&RunEvent::UsageReported {
        slot: Some(0),
        task: "1".into(),
        invocation_id: "inv-1".into(),
        usage: demo_usage(),
    });
    state.apply(&RunEvent::RunLink {
        label: "Dashboard".into(),
        url: "http://127.0.0.1:5000".into(),
    });

    // Wide, narrow-stack, minimal, and tiny sizes across every view. The live
    // slot, usage, and link persist so the live agent block, cost rows, and
    // links section are all exercised.
    for (w, h) in [(120u16, 40u16), (80, 24), (50, 14), (30, 8), (24, 6)] {
        for view in View::ORDER {
            state.view = view;
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|f| super::render::draw(f, &state)).unwrap();
        }
    }

    // Overlays.
    for (help, composer, gate) in [(true, false, false), (false, true, false), (false, false, true)]
    {
        let mut s = state_with_plan();
        s.select_task("2");
        s.help = help;
        if composer {
            s.composer = Some(super::state::Composer {
                task: "1".into(),
                slot: Some(0),
                input: "hi".into(),
            });
        }
        s.gate_active = gate;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| super::render::draw(f, &s)).unwrap();
    }
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
