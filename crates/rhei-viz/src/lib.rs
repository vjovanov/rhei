//! Builds the [`VizModel`] from a parsed plan and its resolved machine — the
//! single source of truth for the spec's derivation rules (flattening, plan
//! state, classification), shared by the static and live paths. §AR-rhei-viz-flow.8

use std::collections::HashSet;

use rhei_core::ast::{Rhei, Task as AstTask};
use rhei_validator::{parse_execution_target, parse_task_state, StateArtifactDef, StateMachine};
use rhei_viz_model::{
    Artifact, Machine, MachineState, TaskRow, TemplateContext, Transition, VizModel,
};

mod collect;
pub use collect::{collect_plans, Bundle};

/// Coarse status category a persisted state reduces to (§FS-rhei-viz §1.1). The
/// rows are evaluated top to bottom, first match wins, so `Active` is the
/// catch-all. The live dashboard overlays runtime slot assignment separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Done,
    Blocked,
    Failed,
    Gate,
    Retired,
    Idle,
    Active,
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
    let parsed = parse_task_state(&task.state, machine);
    out.push(TaskRow {
        id: id.clone(),
        title: task.title.clone(),
        parent,
        depth,
        state: parsed.state,
        visit_count: parsed.visit,
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
                template_context: template_context(def),
                template_contexts: fanout_template_contexts(def),
            }
        })
        .collect();

    Machine { name: machine.name.clone(), states }
}

fn target_template_context(target: rhei_validator::ExecutionTarget) -> TemplateContext {
    TemplateContext {
        target: Some(target.selector()),
        target_slug: Some(target.slug()),
        model_provider: target.provider.clone(),
        model_name: Some(target.model.clone()),
        model: Some(target.model),
        agent: Some(target.agent),
        agent_mode: target.mode,
    }
}

fn model_template_context(def: &rhei_validator::StateDef, model: String) -> TemplateContext {
    TemplateContext {
        model: Some(model.clone()),
        model_name: Some(model),
        agent: def.agent.as_ref().map(|agent| agent.id().to_string()),
        agent_mode: def.agent_mode.clone(),
        ..TemplateContext::default()
    }
}

fn explicit_template_context(def: &rhei_validator::StateDef) -> TemplateContext {
    if let Some(selector) = def.target.as_deref() {
        if let Ok(target) = parse_execution_target(selector) {
            return target_template_context(target);
        }
    }
    if let Some(model) = def.model.as_ref().map(|model| model.trim().to_string()) {
        return model_template_context(def, model);
    }
    TemplateContext {
        agent: def.agent.as_ref().map(|agent| agent.id().to_string()),
        agent_mode: def.agent_mode.clone(),
        ..TemplateContext::default()
    }
}

// Static prompt/artifact previews resolve only authored concrete values; multi
// fanout expands into per-target/model variants instead of guessing. §FS-rhei-viz.8
fn template_context(def: &rhei_validator::StateDef) -> TemplateContext {
    let contexts = authored_fanout_template_contexts(def);
    if contexts.len() == 1 {
        contexts.into_iter().next().unwrap_or_default()
    } else {
        explicit_template_context(def)
    }
}

fn fanout_template_contexts(def: &rhei_validator::StateDef) -> Vec<TemplateContext> {
    let contexts = authored_fanout_template_contexts(def);
    if contexts.len() > 1 {
        contexts
    } else {
        Vec::new()
    }
}

fn authored_fanout_template_contexts(def: &rhei_validator::StateDef) -> Vec<TemplateContext> {
    if !def.all_targets.is_empty() {
        return def
            .all_targets
            .iter()
            .filter_map(|selector| parse_execution_target(selector).ok())
            .map(target_template_context)
            .collect();
    }
    if !def.all_models.is_empty() {
        return def
            .all_models
            .iter()
            .map(|model| model_template_context(def, model.trim().to_string()))
            .collect();
    }
    Vec::new()
}

/// Classify a persisted state into one of the seven categories: machine flags
/// first, state name second; `Active` is the catch-all. Mirrors the asset's
/// `category()`. §FS-rhei-viz.1.1
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
    Category::Active
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
    let any_active_like = roots.iter().any(|s| category(machine, s) == Category::Active);
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
    fn task_visit_and_unambiguous_template_context_are_exposed() {
        let machine = StateMachine::from_yaml_str(
            r#"
name: custom
version: 1.0
states:
  review:
    visits: 3
    target: codex:openai:gpt-5
    instructions: "Review {task_id} in {state}-{visit_count} using {target.slug}"
    outputs:
      - name: notes
        path: runtime/reviews/{task_id}-{state}-{visit_count}-{target.slug}.md
  completed:
    final: true
"#,
        )
        .expect("states load");
        let rhei = parse(
            "# Rhei: Visits\n**States:** custom\n\n## Tasks\n\n### Task 1: A\n**State:** review-2\n",
        )
        .expect("parse");

        let model = build(&rhei, &machine);
        assert_eq!(model.tasks[0].state, "review");
        assert_eq!(model.tasks[0].visit_count, Some(2));
        let review = model.machine.states.iter().find(|s| s.name == "review").unwrap();
        assert_eq!(review.template_context.target.as_deref(), Some("codex:openai:gpt-5"));
        assert_eq!(review.template_context.target_slug.as_deref(), Some("codex-openai-gpt-5"));
        assert_eq!(review.template_context.model.as_deref(), Some("gpt-5"));
        assert_eq!(review.template_context.model_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn multi_target_fanout_contexts_are_exposed_without_guessing() {
        let machine = StateMachine::from_yaml_str(
            r#"
name: custom
version: 1.0
states:
  product-run:
    all_targets:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - codex[xhigh]:openai:gpt-5.5
    instructions: "Write {output.notes.path} for {target}"
    outputs:
      - name: notes
        path: runtime/{target.slug}/{task_id}.md
  completed:
    final: true
"#,
        )
        .expect("states load");
        let rhei = parse(
            "# Rhei: Fanout\n**States:** custom\n\n## Tasks\n\n### Task pm: Evaluate\n**State:** product-run\n",
        )
        .expect("parse");

        let model = build(&rhei, &machine);
        let product = model.machine.states.iter().find(|s| s.name == "product-run").unwrap();
        assert_eq!(product.template_context.target, None);
        assert_eq!(product.template_context.target_slug, None);
        assert_eq!(product.template_contexts.len(), 2);
        assert_eq!(
            product.template_contexts[0].target.as_deref(),
            Some("claude-code[yolo]:anthropic:claude-opus-4-7")
        );
        assert_eq!(
            product.template_contexts[0].target_slug.as_deref(),
            Some("claude-code-yolo-anthropic-claude-opus-4-7")
        );
        assert_eq!(
            product.template_contexts[1].target_slug.as_deref(),
            Some("codex-xhigh-openai-gpt-5.5")
        );
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
