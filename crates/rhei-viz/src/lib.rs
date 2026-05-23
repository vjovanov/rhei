//! Builds the [`VizModel`] from a parsed plan and its resolved machine — the
//! single source of truth for the spec's derivation rules (flattening, plan
//! state, classification), shared by the static and live paths. §AR-rhei-viz-flow.8

use std::collections::HashSet;

use rhei_core::ast::{Rhei, Task as AstTask};
use rhei_validator::{parse_task_state, StateArtifactDef, StateMachine};
use rhei_viz_model::{Artifact, Machine, MachineState, TaskRow, Transition, VizModel};

mod collect;
pub use collect::{collect_plans, Bundle};

/// Coarse status category a state reduces to (§FS-rhei-viz §1.1). The rows are
/// evaluated top to bottom, first match wins, so `Live` is the catch-all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Done,
    Blocked,
    Failed,
    Gate,
    Retired,
    Idle,
    Live,
}

/// Build the static [`VizModel`] from a parsed plan and its resolved machine.
///
/// `tasks` is flattened to source order — each top-level task followed by its
/// descendants — each carrying its tree `depth` (`0` for a top-level task) and
/// `parent` id. The asset renders the outline and graph from this list.
pub fn build(rhei: &Rhei, machine: &StateMachine) -> VizModel {
    let mut tasks = Vec::new();
    for task in &rhei.tasks {
        collect_task(task, 0, None, machine, &mut tasks);
    }
    let plan_state = derive_plan_state(&tasks, machine);
    let about = rhei
        .content_sections
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case("overview"))
        .or_else(|| rhei.content_sections.first())
        .map(|s| s.content.trim().to_string())
        .filter(|s| !s.is_empty());
    VizModel {
        plan_title: Some(rhei.title.clone()),
        plan_state: Some(plan_state),
        about,
        tasks,
        machine: flatten_machine(machine),
    }
}

fn collect_task(
    task: &AstTask,
    depth: u8,
    parent: Option<String>,
    machine: &StateMachine,
    out: &mut Vec<TaskRow>,
) {
    let id = task.id.to_string();
    out.push(TaskRow {
        id: id.clone(),
        title: task.title.clone(),
        parent,
        depth,
        state: normalize_state(&task.state, machine),
        prior: task.prior.iter().map(ToString::to_string).collect(),
    });
    for child in &task.children {
        collect_task(child, depth + 1, Some(id.clone()), machine, out);
    }
}

/// Normalize a raw `**State:**` value through the machine (e.g. resolving the
/// `-N` visit suffix on counted states for a live render).
pub fn normalize_state(raw_state: &str, machine: &StateMachine) -> String {
    parse_task_state(raw_state, machine).state
}

/// The set of states that are the entry of at least one profile, unioned with
/// any state flagged `initial: true` directly. §FS-rhei-viz §8, §FS-rhei-states
fn initial_states(machine: &StateMachine) -> HashSet<String> {
    let mut set: HashSet<String> = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect();
    if let Some(profiles) = &machine.profiles {
        for profile in profiles.values() {
            set.insert(profile.initial.clone());
        }
    }
    set
}

/// Flatten a [`StateMachine`] into the model's [`Machine`]: states in declared
/// order, each with its outgoing transitions (explicit first, then applicable
/// `from: "*"` wildcard edges) and artifact contracts. §FS-rhei-viz.8
pub fn flatten_machine(machine: &StateMachine) -> Machine {
    let initials = initial_states(machine);

    let to_artifacts = |defs: &[StateArtifactDef]| {
        defs.iter()
            .map(|a| Artifact {
                name: a.name.clone(),
                path: a.path.clone(),
                description: a.description.clone(),
                optional: a.optional,
            })
            .collect::<Vec<_>>()
    };

    let states = machine
        .states
        .iter()
        .map(|(name, def)| {
            let mut transitions: Vec<Transition> = machine
                .transitions
                .iter()
                .filter(|rule| rule.from.0 == *name)
                .map(|rule| Transition {
                    to: rule.to.0.clone(),
                    condition: rule.condition.clone(),
                    wildcard: false,
                })
                .collect();
            // Attach `from: "*"` wildcard edges to every non-terminal state so
            // the inspector shows the real set of legal exits.
            if !def.terminal {
                for rule in machine.transitions.iter().filter(|rule| rule.from.0 == "*") {
                    if rule.to.0 != *name && !transitions.iter().any(|t| t.to == rule.to.0) {
                        transitions.push(Transition {
                            to: rule.to.0.clone(),
                            condition: rule.condition.clone(),
                            wildcard: true,
                        });
                    }
                }
            }
            MachineState {
                name: name.clone(),
                description: def.description.clone(),
                instructions: def.instructions.clone(),
                visits: def.visits,
                initial: initials.contains(name),
                terminal: def.terminal,
                gating: def.gating,
                transitions,
                inputs: to_artifacts(&def.inputs),
                outputs: to_artifacts(&def.outputs),
            }
        })
        .collect();

    Machine { name: machine.name.clone(), states }
}

/// Classify a state into one of the seven categories: machine flags first, then
/// the state name (so custom vocabularies classify); first match wins, `Live`
/// the catch-all. Mirrors the asset's `category()`. §FS-rhei-viz.1.1
pub fn category(machine: &StateMachine, state: &str) -> Category {
    let def = machine.states.get(state);
    if state == "completed" {
        return Category::Done;
    }
    if state == "failed" {
        return Category::Failed;
    }
    if state == "blocked" {
        return Category::Blocked;
    }
    if def.map(|d| d.gating).unwrap_or(false) || state == "human-review" {
        return Category::Gate;
    }
    if def.map(|d| d.terminal).unwrap_or(false) {
        return if state == "completed" { Category::Done } else { Category::Retired };
    }
    if state == "cancelled" || state == "archived" {
        return Category::Retired;
    }
    let is_initial =
        def.map(|d| d.initial).unwrap_or(false) || initial_states(machine).contains(state);
    if state == "draft" || state == "pending" || is_initial {
        return Category::Idle;
    }
    Category::Live
}

/// Derive the level-0 plan state from top-level task states: the pure derivation
/// over state names; the live host additionally promotes to `active` when a
/// top-level task is assigned to a running slot. §FS-rhei-viz.9
pub fn derive_plan_state(tasks: &[TaskRow], machine: &StateMachine) -> String {
    let roots: Vec<&str> =
        tasks.iter().filter(|t| t.depth == 0).map(|t| t.state.as_str()).collect();
    if roots.is_empty() {
        return "draft".into();
    }
    if roots.iter().all(|s| *s == "draft") {
        return "draft".into();
    }
    if roots.iter().all(|s| *s == "completed") {
        return "completed".into();
    }

    let terminal: HashSet<&str> = machine
        .states
        .iter()
        .filter_map(|(name, def)| def.terminal.then_some(name.as_str()))
        .collect();
    if roots.iter().all(|s| terminal.contains(s)) {
        return "archived".into();
    }

    // active-like = a non-terminal state that is not in the `idle` category.
    let any_active_like = roots.iter().any(|s| category(machine, s) == Category::Live);
    if any_active_like {
        "active".into()
    } else {
        "pending".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhei_core::parse;
    use rhei_validator::StateMachine;

    fn builtin() -> StateMachine {
        StateMachine::builtin_default()
    }

    #[test]
    fn flat_tasks_carry_depth_and_parent() {
        let rhei = parse(
            "# Rhei: Deep\n**States:** rhei\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [task, bug]\n---\n\n## Tasks\n\n### Task api: Build API\n**State:** pending\n\n#### Bug api.cache: Cache issue\n**State:** in-progress\n",
        )
        .expect("parse");
        let model = build(&rhei, &builtin());
        assert_eq!(model.tasks.len(), 2);
        assert_eq!(model.tasks[0].id, "api");
        assert_eq!(model.tasks[0].depth, 0);
        assert_eq!(model.tasks[0].parent, None);
        assert_eq!(model.tasks[1].id, "api.cache");
        assert_eq!(model.tasks[1].depth, 1);
        assert_eq!(model.tasks[1].parent.as_deref(), Some("api"));
    }

    #[test]
    fn plan_state_pending_when_only_pending_roots() {
        let rhei = parse(
            "# Rhei: P\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n\n### Task 2: B\n**State:** pending\n",
        )
        .expect("parse");
        let model = build(&rhei, &builtin());
        assert_eq!(model.plan_state.as_deref(), Some("pending"));
    }

    #[test]
    fn plan_state_active_when_a_root_is_active_like() {
        let rhei = parse(
            "# Rhei: A\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** in-progress\n\n### Task 2: B\n**State:** pending\n",
        )
        .expect("parse");
        let model = build(&rhei, &builtin());
        assert_eq!(model.plan_state.as_deref(), Some("active"));
    }

    #[test]
    fn plan_state_completed_and_archived() {
        let completed = parse(
            "# Rhei: C\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** completed\n",
        )
        .expect("parse");
        assert_eq!(build(&completed, &builtin()).plan_state.as_deref(), Some("completed"));

        let archived = parse(
            "# Rhei: C\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** completed\n\n### Task 2: B\n**State:** cancelled\n",
        )
        .expect("parse");
        assert_eq!(build(&archived, &builtin()).plan_state.as_deref(), Some("archived"));
    }

    #[test]
    fn machine_flattening_marks_wildcard_and_initial() {
        let machine = builtin();
        let flat = flatten_machine(&machine);
        // The built-in `default-rhei` profile enters at `draft`, so the
        // profile-initial union must mark `draft` initial.
        let draft = flat.states.iter().find(|s| s.name == "draft").expect("draft state");
        assert!(draft.initial, "draft is the built-in profile's initial state");
        let completed = flat.states.iter().find(|s| s.name == "completed").expect("completed");
        assert!(completed.terminal);
        assert!(completed.transitions.is_empty(), "terminal states get no wildcard exits");
    }
}
