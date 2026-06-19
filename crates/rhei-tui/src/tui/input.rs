//! Keyboard handling for the Flow surface: the two-level selection model, view
//! switching, filters, and the live intervene/gate composers.
//! §FS-rhei-run-tui.1.5.2 §FS-rhei-run-tui.1.5.5

use crossterm::event::{KeyCode, KeyModifiers};

use crate::event::MessageLevel;

use super::derive::{inspector_chips, ChipAction};
use super::state::{Composer, FlowFocus, UiState, View};

/// What the render loop should do after a key event.
pub(super) enum InputAction {
    Continue,
    ForwardSigint,
    Quit,
}

pub(super) fn handle_key_event(
    state: &mut UiState,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> InputAction {
    // Ctrl+C always restores the terminal and re-raises SIGINT (§1.8).
    if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
        state.push_journal(MessageLevel::Info, "(ctrl+c received — forwarding SIGINT)".to_string());
        return InputAction::ForwardSigint;
    }

    // Modal inputs intercept everything else.
    if state.composer.is_some() {
        return handle_composer(state, code);
    }
    if state.gate_active {
        return handle_gate(state, code);
    }
    if state.filter_editing {
        return handle_filter(state, code);
    }
    if state.help {
        if matches!(code, KeyCode::Char('?') | KeyCode::Esc) {
            state.help = false;
        }
        return InputAction::Continue;
    }

    match code {
        KeyCode::Char('?') => state.help = true,
        KeyCode::Char('q') => {
            // Quit only once the run has finished; during a live run the
            // operator stops with Ctrl+C (§1.5.2).
            if state.finished {
                return InputAction::Quit;
            }
        }
        KeyCode::Char('1') => switch_view(state, View::Flow),
        KeyCode::Char('2') => switch_view(state, View::Machine),
        KeyCode::Char('3') => switch_view(state, View::Cost),
        KeyCode::Char('4') => switch_view(state, View::Journal),
        KeyCode::Char('5') => switch_view(state, View::Tasks),
        KeyCode::Char('h') | KeyCode::Left => cycle_view(state, -1),
        KeyCode::Char('l') | KeyCode::Right => cycle_view(state, 1),
        KeyCode::Char('j') | KeyCode::Down => move_focus(state, 1),
        KeyCode::Char('k') | KeyCode::Up => move_focus(state, -1),
        KeyCode::PageDown => move_focus(state, 10),
        KeyCode::PageUp => move_focus(state, -10),
        KeyCode::Tab => {
            if state.view == View::Flow {
                state.flow_focus = match state.flow_focus {
                    FlowFocus::Outline => FlowFocus::Inspector,
                    FlowFocus::Inspector => FlowFocus::Outline,
                };
            }
        }
        KeyCode::Enter => handle_enter(state),
        KeyCode::Char('/') => {
            state.filter_editing = true;
            state.filter = Some(state.filter.clone().unwrap_or_default());
        }
        KeyCode::Char('g') => {
            if state.view == View::Cost {
                state.cost_group = state.cost_group.next();
                state.cost_cursor = 0;
            }
        }
        KeyCode::Char('s') => {
            if state.view == View::Tasks {
                state.tasks_sort = state.tasks_sort.next();
            }
        }
        KeyCode::Char('f') => match state.view {
            View::Journal => {
                state.journal_filter = state.journal_filter.next();
                state.journal_scroll = 0;
            }
            View::Tasks => state.cycle_tasks_state_filter(),
            _ => {}
        },
        KeyCode::Char('m') => open_composer(state),
        _ => {}
    }

    InputAction::Continue
}

fn switch_view(state: &mut UiState, view: View) {
    state.view = view;
    if view == View::Flow {
        state.flow_focus = FlowFocus::Outline;
    }
}

fn cycle_view(state: &mut UiState, delta: isize) {
    let idx = state.view.index() as isize;
    let len = View::ORDER.len() as isize;
    let next = ((idx + delta) % len + len) % len;
    switch_view(state, View::ORDER[next as usize]);
}

fn move_focus(state: &mut UiState, delta: isize) {
    match state.view {
        View::Flow => match state.flow_focus {
            FlowFocus::Outline => {
                let order = state.visible_task_indices();
                state.move_selected_in(&order, delta);
            }
            FlowFocus::Inspector => {
                let chips = state
                    .selected
                    .clone()
                    .map(|id| inspector_chips(state, &id))
                    .unwrap_or_default();
                if chips.is_empty() {
                    state.inspector_scroll = clamp_scroll(state.inspector_scroll, delta);
                } else {
                    let cur = state.inspector_chip as isize;
                    state.inspector_chip =
                        (cur + delta).clamp(0, chips.len() as isize - 1) as usize;
                }
            }
        },
        View::Machine => {
            let order = state.machine_view_order();
            if !order.is_empty() {
                let current = order.iter().position(|idx| *idx == state.machine_focus).unwrap_or(0);
                let next = (current as isize + delta).clamp(0, order.len() as isize - 1);
                state.machine_focus = order[next as usize];
            }
        }
        View::Cost => {
            if matches!(state.cost_group, super::state::CostGroup::Task) {
                let order = state.visible_task_indices();
                state.move_selected_in(&order, delta);
            } else {
                state.cost_cursor = (state.cost_cursor as isize + delta).max(0) as usize;
            }
        }
        View::Tasks => {
            let order = state.tasks_view_order();
            state.move_selected_in(&order, delta);
        }
        View::Journal => {
            state.journal_scroll = clamp_scroll(state.journal_scroll, delta);
        }
    }
}

fn clamp_scroll(current: u16, delta: isize) -> u16 {
    (current as isize + delta).max(0) as u16
}

fn handle_enter(state: &mut UiState) {
    match state.view {
        View::Tasks => {
            // Enter on a row returns to Flow with that task selected.
            switch_view(state, View::Flow);
        }
        View::Flow if state.flow_focus == FlowFocus::Outline => {
            // Enter on a gating task opens the human-gate chooser (§1.5.5).
            open_gate(state);
        }
        View::Flow => {
            let Some(id) = state.selected.clone() else { return };
            let chips = inspector_chips(state, &id);
            if let Some(chip) = chips.get(state.inspector_chip) {
                match &chip.action {
                    ChipAction::SelectTask(target) => {
                        state.select_task(target);
                    }
                    ChipAction::MarkState(target) => {
                        // Mark the target state in the Machine view, keeping the
                        // selected task in context.
                        if let Some(pos) =
                            state.plan.machine.states.iter().position(|s| &s.name == target)
                        {
                            state.machine_focus = pos;
                            switch_view(state, View::Machine);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_filter(state: &mut UiState, code: KeyCode) -> InputAction {
    match code {
        KeyCode::Esc => {
            state.filter = None;
            state.filter_editing = false;
            state.reconcile_filter_focus();
        }
        KeyCode::Enter => {
            state.filter_editing = false;
            if state.filter.as_deref() == Some("") {
                state.filter = None;
            }
            state.reconcile_filter_focus();
        }
        KeyCode::Backspace => {
            if let Some(f) = state.filter.as_mut() {
                f.pop();
            }
        }
        KeyCode::Char(c) => {
            state.filter.get_or_insert_with(String::new).push(c);
        }
        _ => {}
    }
    InputAction::Continue
}

/// Open the intervene composer for the selected live task, when its agent is
/// reachable. Otherwise leave a journal note naming the remedy.
/// §FS-rhei-run-tui.1.5.5
fn open_composer(state: &mut UiState) {
    if state.finished {
        return;
    }
    let Some(id) = state.selected.clone() else { return };
    let Some((slot, _)) = state.running_slot(&id) else {
        return;
    };
    let reachable =
        state.intervene.as_ref().map(|sink| sink.reachable(&id, Some(slot))).unwrap_or(false);
    if !reachable {
        state.push_journal(
            MessageLevel::Warn,
            format!("{id}: agent is not reachable — set intervene_stdin and rerun"),
        );
        return;
    }
    state.composer = Some(Composer { task: id, slot: Some(slot), input: String::new() });
}

fn handle_composer(state: &mut UiState, code: KeyCode) -> InputAction {
    match code {
        KeyCode::Esc => {
            state.composer = None;
        }
        KeyCode::Backspace => {
            if let Some(c) = state.composer.as_mut() {
                c.input.pop();
            }
        }
        KeyCode::Char(ch) => {
            if let Some(c) = state.composer.as_mut() {
                c.input.push(ch);
            }
        }
        KeyCode::Enter => {
            let Some(composer) = state.composer.take() else {
                return InputAction::Continue;
            };
            let message = composer.input.trim().to_string();
            if message.is_empty() {
                return InputAction::Continue;
            }
            let result = match &state.intervene {
                Some(sink) => sink.deliver(Some(&composer.task), composer.slot, &message),
                None => Err("intervene is not available".to_string()),
            };
            match result {
                Ok(()) => state.push_journal(
                    MessageLevel::Info,
                    format!("⌨ intervene → {}: {message}", composer.task),
                ),
                Err(reason) => state.push_journal(
                    MessageLevel::Warn,
                    format!("intervene to {} failed: {reason}", composer.task),
                ),
            }
        }
        _ => {}
    }
    InputAction::Continue
}

/// Submit a human-gate transition: the digit keys pick one of the gating
/// state's explicit outgoing transitions. §FS-rhei-run-tui.1.5.5
fn handle_gate(state: &mut UiState, code: KeyCode) -> InputAction {
    match code {
        KeyCode::Esc => {
            state.gate_active = false;
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            let Some(choice) = ch.to_digit(10) else {
                return InputAction::Continue;
            };
            if choice == 0 {
                return InputAction::Continue;
            }
            let choices = gate_choices(state);
            if let Some((from, to)) = choices.get((choice - 1) as usize).cloned() {
                let Some(id) = state.selected.clone() else {
                    return InputAction::Continue;
                };
                let result = match &state.gate {
                    Some(sink) => sink.transition_gate(&id, &from, &to),
                    None => Err("gate transitions are not available".to_string()),
                };
                match result {
                    Ok(effective) => state.push_journal(
                        MessageLevel::Info,
                        format!("⮞ gate {id}: {from}→{effective}"),
                    ),
                    Err(reason) => state.push_journal(
                        MessageLevel::Warn,
                        format!("gate {id} {from}→{to} rejected: {reason}"),
                    ),
                }
            }
            state.gate_active = false;
        }
        _ => {}
    }
    InputAction::Continue
}

/// The explicit outgoing `(from, to)` transitions for the selected task's
/// current gating state.
pub(super) fn gate_choices(state: &UiState) -> Vec<(String, String)> {
    let Some(task) = state.selected_task() else {
        return Vec::new();
    };
    let Some(st) = state.machine_state(&task.state) else {
        return Vec::new();
    };
    if !st.gating {
        return Vec::new();
    }
    st.transitions.iter().filter(|t| !t.wildcard).map(|t| (st.name.clone(), t.to.clone())).collect()
}

/// Whether the `m` intervene action currently applies (selected task is live).
pub(super) fn intervene_available(state: &UiState) -> bool {
    !state.finished && state.selected.as_ref().map(|id| state.is_live(id)).unwrap_or(false)
}

/// Open the gate chooser for the selected task when it sits in a live gating
/// state. Returns whether it opened (used by the action bar key).
pub(super) fn open_gate(state: &mut UiState) -> bool {
    if state.finished {
        return false;
    }
    if gate_choices(state).is_empty() || state.gate.is_none() {
        return false;
    }
    state.gate_active = true;
    true
}
