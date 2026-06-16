//! Pure derivations over the run model: the inspector's followable chips, the
//! machine's disjoint-workflow grouping, and cost rollups — shared by input and
//! rendering so a chip navigates exactly where it is drawn. §FS-rhei-run-tui.1.5

use rhei_viz_model::{Machine, TaskRow, VizModel};

use super::state::{UiState, UsageRecord};

/// What following an inspector chip does (§FS-rhei-run-tui.1.5.2): select a
/// neighbor task, or mark a target state in the Machine view.
#[derive(Clone)]
pub(super) enum ChipAction {
    SelectTask(String),
    MarkState(String),
}

#[derive(Clone)]
pub(super) struct Chip {
    pub(super) label: String,
    pub(super) action: ChipAction,
}

/// The followable chips of the selected task's surroundings inspector, in
/// inspector order: depends-on, unblocks, came-from, next-state
/// (§FS-rhei-viz.4). Walking from a node to a neighbor costs one keystroke.
pub(super) fn inspector_chips(state: &UiState, task_id: &str) -> Vec<Chip> {
    let plan = &state.plan;
    let Some(task) = state.task(task_id) else {
        return Vec::new();
    };
    let mut chips = Vec::new();

    // Depends on (Prior).
    for prior in &task.prior {
        chips.push(Chip {
            label: format!("◂ {prior}"),
            action: ChipAction::SelectTask(prior.clone()),
        });
    }
    // Unblocks: tasks waiting on this one.
    for other in &plan.tasks {
        if other.prior.iter().any(|p| p == task_id) {
            chips.push(Chip {
                label: format!("▸ {}", other.id),
                action: ChipAction::SelectTask(other.id.clone()),
            });
        }
    }

    if let Some(machine_state) = state.machine_state(&task.state) {
        // Came from: states that can transition into this state.
        for st in &plan.machine.states {
            if st.transitions.iter().any(|t| t.to == machine_state.name) {
                chips.push(Chip {
                    label: format!("⮜ {}", st.name),
                    action: ChipAction::MarkState(st.name.clone()),
                });
            }
        }
        // Next state: this state's outgoing transitions.
        for tr in &machine_state.transitions {
            chips.push(Chip {
                label: format!("⮞ {}", tr.to),
                action: ChipAction::MarkState(tr.to.clone()),
            });
        }
    }

    chips
}

/// Group machine states into disjoint workflows: connected components over
/// explicit (non-wildcard) transitions, ordered by declaration. Isolated states
/// and wildcard-terminal states fold into a trailing group. §FS-rhei-run-tui.1.5.4
pub(super) fn machine_groups(machine: &Machine) -> Vec<Vec<usize>> {
    let n = machine.states.len();
    if n == 0 {
        return Vec::new();
    }
    let index_of = |name: &str| machine.states.iter().position(|s| s.name == name);

    // Union-find over explicit edges.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let union = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    };
    let mut connected = vec![false; n];
    for (i, st) in machine.states.iter().enumerate() {
        for tr in &st.transitions {
            if tr.wildcard {
                continue;
            }
            if let Some(j) = index_of(&tr.to) {
                union(&mut parent, i, j);
                connected[i] = true;
                connected[j] = true;
            }
        }
    }

    // Multi-state components first, in declaration order of their first member.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut seen_root: Vec<Option<usize>> = vec![None; n];
    let connected_snapshot = connected.clone();
    for (i, is_connected) in connected_snapshot.iter().enumerate() {
        if !is_connected {
            continue;
        }
        let root = find(&mut parent, i);
        match seen_root[root] {
            Some(g) => groups[g].push(i),
            None => {
                seen_root[root] = Some(groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups.retain(|g| g.len() > 1);

    // Trailing group: isolated + wildcard-only-terminal states.
    let mut trailing: Vec<usize> =
        (0..n).filter(|i| !groups.iter().any(|g| g.contains(i))).collect();
    trailing.sort_unstable();
    if !trailing.is_empty() {
        groups.push(trailing);
    }
    groups
}

/// Compact per-key cost rollup for the Cost view.
#[derive(Default, Clone)]
pub(super) struct CostRollup {
    pub(super) cost_micro: Option<u64>,
    pub(super) any_cost_missing: bool,
    pub(super) total_tokens: u64,
    pub(super) input_tokens: u64,
    pub(super) input_cached_read_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) output_cached_read_tokens: u64,
    pub(super) invocations: u64,
}

impl CostRollup {
    pub(super) fn add(&mut self, usage: &crate::event::UsageSummary) {
        self.invocations += 1;
        match usage.cost_micro.or(usage.priced_cost_micro) {
            Some(c) => self.cost_micro = Some(self.cost_micro.unwrap_or(0) + c),
            None => self.any_cost_missing = true,
        }
        self.total_tokens += usage.total.value.unwrap_or(0);
        self.input_tokens += usage.input_total.value.unwrap_or(0);
        self.input_cached_read_tokens += usage.input_cached_read.value.unwrap_or(0);
        self.output_tokens += usage.output_total.value.unwrap_or(0);
        self.output_cached_read_tokens += usage.output_cached_read.value.unwrap_or(0);
    }

    /// A coverage glyph — meaning never rides color alone (§FS-rhei-cost-accounting).
    pub(super) fn coverage_glyph(&self) -> char {
        if self.invocations == 0 {
            '·'
        } else if self.any_cost_missing {
            '◍'
        } else {
            '✓'
        }
    }
}

/// Run-level rollup across every recorded invocation.
pub(super) fn run_rollup(invocations: &[UsageRecord]) -> CostRollup {
    let mut roll = CostRollup::default();
    for rec in invocations {
        roll.add(&rec.usage);
    }
    roll
}

/// Direct cost of one task: invocations recorded against that exact id.
pub(super) fn task_direct(invocations: &[UsageRecord], task_id: &str) -> CostRollup {
    let mut roll = CostRollup::default();
    for rec in invocations.iter().filter(|r| r.task == task_id) {
        roll.add(&rec.usage);
    }
    roll
}

/// Subtree cost: the task plus every id-descendant (`<id>.` prefix).
pub(super) fn task_subtree(
    plan: &VizModel,
    invocations: &[UsageRecord],
    task_id: &str,
) -> CostRollup {
    let prefix = format!("{task_id}.");
    let ids: Vec<&str> = plan
        .tasks
        .iter()
        .filter(|t| t.id == task_id || t.id.starts_with(&prefix))
        .map(|t| t.id.as_str())
        .collect();
    let mut roll = CostRollup::default();
    for rec in invocations.iter().filter(|r| ids.contains(&r.task.as_str())) {
        roll.add(&rec.usage);
    }
    roll
}

/// Whether a task has any descendant.
pub(super) fn has_children(plan: &VizModel, task: &TaskRow) -> bool {
    let prefix = format!("{}.", task.id);
    plan.tasks.iter().any(|t| t.id.starts_with(&prefix))
}

/// `done/total` over a task's descendants (completed leaves vs all). Returns
/// `None` for a leaf.
pub(super) fn subtree_progress(plan: &VizModel, task: &TaskRow) -> Option<(usize, usize)> {
    let prefix = format!("{}.", task.id);
    let descendants: Vec<&TaskRow> =
        plan.tasks.iter().filter(|t| t.id.starts_with(&prefix)).collect();
    if descendants.is_empty() {
        return None;
    }
    let done = descendants.iter().filter(|t| t.state == "completed").count();
    Some((done, descendants.len()))
}
