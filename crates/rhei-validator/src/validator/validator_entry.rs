/// Parsed task-state value from markdown, optionally carrying an explicit visit suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTaskState {
    /// Canonical state name defined in the state machine.
    pub state: String,
    /// Explicit visit count encoded in markdown as `<state>-<n>`.
    pub visit: Option<u32>,
}

/// Parse a markdown task-state value against a state machine.
///
/// Exact state names take precedence. If the raw value is not an exact state
/// match, Rhei interprets a trailing `-<n>` suffix as a counted-loop visit when
/// the prefix is a valid state name.
pub fn parse_task_state(raw: &str, machine: &StateMachine) -> ParsedTaskState {
    if machine.is_valid_state(raw) {
        return ParsedTaskState { state: raw.to_string(), visit: None };
    }

    if let Some((base, visit_text)) = raw.rsplit_once('-') {
        if let Ok(visit) = visit_text.parse::<u32>() {
            if machine.is_valid_state(base) {
                return ParsedTaskState { state: base.to_string(), visit: Some(visit) };
            }
        }
    }

    ParsedTaskState { state: raw.to_string(), visit: None }
}

// ========================================
// Semantic Validator (Task 5)
// ========================================

/// Validator configured with a loaded [`StateMachine`].
pub struct Validator {
    machine: StateMachine,
}

impl Validator {
    /// Create a validator that will use `machine` for allowed-state checks.
    pub fn new(machine: StateMachine) -> Self {
        Self { machine }
    }

    /// Validate a parsed rhei using the currently configured states.
    ///
    /// This does not check markdown link targets (no file-system context).
    /// Use [`validate_with_base`](Self::validate_with_base) to also verify links.
    pub fn validate(&self, rhei: &Rhei) -> ValidationReport {
        self.validate_with_base(rhei, None)
    }

    /// Validate a parsed rhei, optionally resolving markdown links relative
    /// to `base_path` (the directory containing the plan file).
    pub fn validate_with_base(&self, rhei: &Rhei, base_path: Option<&Path>) -> ValidationReport {
        let mut report = ValidationReport::ok();

        let index = build_task_index(rhei);
        validate_node_policy_against_structure(&self.machine, &rhei.structure, &mut report);
        validate_state_machine_warnings(&self.machine, &mut report);
        validate_sibling_uniqueness(rhei, &mut report);
        validate_dependency_integrity(rhei, &index, &mut report);
        validate_prior_order_coherence(rhei, &index, &self.machine, &mut report);
        validate_state_consistency(rhei, &self.machine, &mut report);
        validate_task_execution_overrides(rhei, &self.machine, &mut report);
        validate_terminal_tree_coherence(rhei, &self.machine, &mut report);
        validate_circular_dependencies(rhei, &index, &mut report);
        validate_assignee_nonempty(rhei, &mut report);
        validate_result_blocks(rhei, &self.machine, &mut report);

        if let Some(base) = base_path {
            validate_markdown_links(rhei, base, &mut report);
        }

        report
    }
}

/// Validate a parsed rhei using an already-loaded [`StateMachine`].
pub fn validate_with_machine(rhei: &Rhei, machine: &StateMachine) -> ValidationReport {
    Validator::new(machine.clone()).validate(rhei)
}

/// Validate a parsed rhei using an already-loaded [`StateMachine`], resolving
/// markdown links relative to `base_path`.
pub fn validate_with_machine_and_base(
    rhei: &Rhei,
    machine: &StateMachine,
    base_path: &Path,
) -> ValidationReport {
    Validator::new(machine.clone()).validate_with_base(rhei, Some(base_path))
}

/// Validate a parsed rhei using per-task markdown link bases. §AR-rhei-panta.5
pub fn validate_with_machine_and_link_bases(
    rhei: &Rhei,
    machine: &StateMachine,
    default_base: &Path,
    task_bases: &HashMap<String, PathBuf>,
    section_bases: &[PathBuf],
) -> ValidationReport {
    let mut report = Validator::new(machine.clone()).validate_with_base(rhei, None);
    validate_markdown_links_with_task_bases(rhei, default_base, task_bases, section_bases, &mut report);
    report
}

/// Load a [`StateMachine`] from `machine_path` and validate a parsed rhei.
pub fn validate_from_machine_file<P: AsRef<Path>>(
    rhei: &Rhei,
    machine_path: P,
) -> Result<ValidationReport, StateMachineLoadError> {
    let machine = StateMachine::from_yaml_file(machine_path)?;
    Ok(Validator::new(machine).validate(rhei))
}

fn validate_state_machine_warnings(machine: &StateMachine, report: &mut ValidationReport) {
    for (state_name, state) in &machine.states {
        if state.gating && state.agent.is_some() {
            report.warnings.push(format!(
                "state '{state_name}' declares 'agent' on a gating state; gating states are human-only, so rhei run will not invoke this agent"
            ));
        }
    }
}

// ---------------------------
// Validation helpers
// ---------------------------

fn build_task_index(rhei: &Rhei) -> HashMap<TaskId, &Task> {
    fn visit<'a>(task: &'a Task, map: &mut HashMap<TaskId, &'a Task>) {
        map.insert(task.id.clone(), task);
        for child in &task.children {
            visit(child, map);
        }
    }
    let mut map = HashMap::new();
    for t in &rhei.tasks {
        visit(t, &mut map);
    }
    map
}

// §FS-rhei-states.9.3: Validate node policy selectors against plan structure.

/// Validate the plan-dependent parts of `node_policy`: by-type keys and
/// override selectors are checked against the current plan's
/// `structure.nodeKinds` and `structure.maxLevels`.
fn validate_node_policy_against_structure(
    machine: &StateMachine,
    structure: &Structure,
    report: &mut ValidationReport,
) {
    let Some(policy) = machine.node_policy.as_ref() else {
        return;
    };

    for kind in policy.by_type.keys() {
        if !structure.accepts_kind(kind) {
            report.errors.push(format!(
                "node_policy.by_type references node kind '{}' but the plan structure declares nodeKinds {:?}",
                kind, structure.node_kinds
            ));
        }
    }

    for (idx, ov) in policy.overrides.iter().enumerate() {
        if let Some(node_type) = ov.match_.node_type.as_deref() {
            if !structure.accepts_kind(node_type) {
                report.errors.push(format!(
                    "node_policy.overrides[{idx}].match.type references node kind '{}' but the plan structure declares nodeKinds {:?}",
                    node_type, structure.node_kinds
                ));
            }
        }
        if let Some(level) = ov.match_.level {
            if level == 0 || level > structure.max_levels {
                report.errors.push(format!(
                    "node_policy.overrides[{idx}].match.level is {}, but levels must be in 1..={} for this plan structure",
                    level, structure.max_levels
                ));
            }
        }
    }
}

/// Call `f` for every node in the tree, depth-first.
fn for_each_node<'a>(rhei: &'a Rhei, mut f: impl FnMut(&'a Task)) {
    fn recurse<'a>(task: &'a Task, f: &mut impl FnMut(&'a Task)) {
        f(task);
        for child in &task.children {
            recurse(child, f);
        }
    }
    for t in &rhei.tasks {
        recurse(t, &mut f);
    }
}

fn validate_dependency_integrity(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    let rhei_ids = project_rhei_ids(rhei);

    fn recurse(
        task: &Task,
        ancestors: &mut Vec<TaskId>,
        index: &HashMap<TaskId, &Task>,
        rhei_ids: &[String],
        structure: &Structure,
        report: &mut ValidationReport,
    ) {
        let mut seen: HashSet<&TaskId> = HashSet::new();
        for (position, dep) in task.prior.iter().enumerate() {
            let kind = task.prior_kinds.get(position).and_then(|k| k.as_deref());
            // A repeated reference is at best noise and at worst a leftover
            // from an edit that meant to name a different task.
            // §FS-rhei-plan-language.3.1
            if !seen.insert(dep) {
                report.errors.push(format!(
                    "Task {} lists Task {} more than once in **Prior:**; drop the duplicate",
                    task.id, dep
                ));
            }
            match (index.get(dep), kind) {
                // The kind keyword is decoration for the reader, so a wrong
                // one misleads exactly where it was meant to help.
                // §FS-rhei-plan-language.3.1
                (Some(target), Some(kind)) if !target.kind.eq_ignore_ascii_case(kind) => {
                    let flavor = if structure.accepts_kind(kind) {
                        String::new()
                    } else {
                        format!(
                            " ('{kind}' is not a declared node kind; declared: {:?})",
                            structure.node_kinds
                        )
                    };
                    report.errors.push(format!(
                        "Task {} **Prior:** kind keyword '{kind}' does not match Task {}: \
                         that node is declared '{}'{flavor}. Use the node's kind or the bare id",
                        task.id,
                        dep,
                        title_case_kind(&target.kind),
                    ));
                }
                // An unknown kind on an unresolvable reference is the shape a
                // pasted task *title* takes — lead with that common mistake.
                // §FS-rhei-plan-language.3.1
                (None, Some(kind)) if !structure.accepts_kind(kind) => {
                    report.errors.push(format!(
                        "Task {} has an unresolvable **Prior:** reference: '{kind}' is not a \
                         declared node kind (declared: {:?}) and no Task {} exists. If the \
                         reference is a task title, use the task's id instead (`**Prior:** 1`, \
                         `**Prior:** auth.2`)",
                        task.id, structure.node_kinds, dep
                    ));
                }
                (None, _) => {
                    report.errors.push(format!(
                        "Task {} depends on missing Task {}{}",
                        task.id,
                        dep,
                        missing_prior_hint(&task.id, dep, index, rhei_ids)
                    ));
                }
                _ => {}
            }
            if ancestors.iter().any(|ancestor| ancestor == dep) {
                report.errors.push(format!(
                    "Task {} cannot list ancestor Task {} as **Prior:**; parent/child structure already defines containment. Make the dependent work a top-level sibling if it must wait for Task {}.",
                    task.id, dep, dep
                ));
            }
        }
        ancestors.push(task.id.clone());
        for child in &task.children {
            recurse(child, ancestors, index, rhei_ids, structure, report);
        }
        ancestors.pop();
    }

    let mut ancestors = Vec::new();
    for task in &rhei.tasks {
        recurse(task, &mut ancestors, index, &rhei_ids, &rhei.structure, report);
    }
}

/// Rhei ids of the merged project: the leading segment of every top-level id.
fn project_rhei_ids(rhei: &Rhei) -> Vec<String> {
    let mut ids: Vec<String> = rhei
        .tasks
        .iter()
        .filter_map(|task| match task.id.segments.first() {
            Some(TaskIdSegment::Named(name)) => Some(name.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Explain a missing prior that resolved to no rhei at all.
///
/// A dotted `**Prior:**` whose leading segment names no rhei is kept as the
/// author wrote it, so the id in the error is quotable back to the source. It is
/// still ambiguous — a typo'd rhei name or a typo'd local hierarchical id — so
/// the hint rules out both readings and only offers a correction that resolves.
// §FS-rhei-validate.4.1: an unresolved prior is reported as the author wrote it.
fn missing_prior_hint(
    task: &TaskId,
    dep: &TaskId,
    index: &HashMap<TaskId, &Task>,
    rhei_ids: &[String],
) -> String {
    let Some(TaskIdSegment::Named(candidate)) = dep.segments.first() else {
        return String::new();
    };
    // A dep under a known rhei is a plain missing ticket and needs no explaining.
    if rhei_ids.iter().any(|id| id == candidate) {
        return String::new();
    }
    let citing_rhei = match task.segments.first() {
        Some(TaskIdSegment::Named(name)) => name.as_str(),
        _ => return String::new(),
    };
    let mut hint = format!(
        ": no rhei named '{candidate}' in this project (rheis: {}), \
         and rhei '{citing_rhei}' has no ticket '{dep}'",
        rhei_ids.join(", ")
    );
    if let Some(corrected) = nearest_resolving_id(task, dep, candidate, index, rhei_ids) {
        hint.push_str(&format!(". Did you mean '{corrected}'?"));
    }
    hint
}

/// Correct `dep`'s leading segment to the nearest rhei id, keeping the
/// suggestion only when it names a real ticket other than the citing one.
///
/// Suggesting an id that does not resolve trades one dead end for another, and
/// suggesting the citing task itself proposes a self-dependency.
fn nearest_resolving_id(
    task: &TaskId,
    dep: &TaskId,
    candidate: &str,
    index: &HashMap<TaskId, &Task>,
    rhei_ids: &[String],
) -> Option<TaskId> {
    let nearest = nearest_rhei_id(candidate, rhei_ids)?;
    let mut segments = dep.segments.clone();
    segments[0] = TaskIdSegment::Named(nearest.to_string());
    let corrected = TaskId::from_segments(segments);
    (corrected != *task && index.contains_key(&corrected)).then_some(corrected)
}

/// Closest rhei id to `candidate` within a small edit distance, if any.
fn nearest_rhei_id<'a>(candidate: &str, rhei_ids: &'a [String]) -> Option<&'a str> {
    // Below three characters every id is within one edit of every other, so a
    // "near miss" carries no signal; above it, two edits catches transpositions
    // and a single slip without pairing unrelated names.
    let length = candidate.chars().count();
    if length < 3 {
        return None;
    }
    let budget = 2.min(length.div_ceil(3)).max(1);
    rhei_ids
        .iter()
        .map(|id| (edit_distance(candidate, id), id.as_str()))
        .filter(|(distance, _)| *distance <= budget)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, id)| id)
}

/// Levenshtein distance over chars.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, a_char) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != b_char);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// Warn about tickets that went terminal while a `**Prior:**` is unsatisfied.
/// A terminal ticket leaves readiness and `--blocked`, so nothing else reveals
/// it; legitimate authoring reaches it, so warn. §FS-rhei-validate.4
fn validate_prior_order_coherence(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    let satisfied = |id: &TaskId| -> bool {
        index
            .get(id)
            .map(|dep| {
                let state = parse_task_state(dep.state.as_str(), machine).state;
                state != "cancelled"
                    && machine.states.get(&state).map(|def| def.terminal).unwrap_or(false)
            })
            .unwrap_or(false)
    };

    for_each_node(rhei, |task| {
        let state = parse_task_state(task.state.as_str(), machine).state;
        // Only a *successful* terminal state is a contradiction: a cancelled
        // ticket never claimed its prerequisites ran.
        if state == "cancelled"
            || !machine.states.get(&state).map(|def| def.terminal).unwrap_or(false)
        {
            return;
        }
        // A missing prior is already a hard error in dependency integrity;
        // do not double-report it here.
        let unmet: Vec<String> = task
            .prior
            .iter()
            .filter(|dep| index.contains_key(*dep) && !satisfied(dep))
            .map(|dep| {
                format!("Task {} ({})", dep, parse_task_state(index[dep].state.as_str(), machine).state)
            })
            .collect();
        if !unmet.is_empty() {
            report.warnings.push(format!(
                "{} {} is '{}' but its prerequisites are unsatisfied: {}. The plan contradicts its own **Prior:** dependencies.",
                title_case_kind(&task.kind),
                task.id,
                state,
                unmet.join(", ")
            ));
        }
    });
}

fn validate_state_consistency(rhei: &Rhei, machine: &StateMachine, report: &mut ValidationReport) {
    for_each_node(rhei, |task| {
        let kind_label = title_case_kind(&task.kind);
        let subject = format!("{} {}", kind_label, task.id);
        validate_task_state_instance(&subject, &task.state, machine, report);
        validate_task_state_against_profile(
            &subject,
            &task.state,
            task.kind.as_str(),
            task.profile_level(),
            machine,
            report,
        );
    });
}

fn validate_task_execution_overrides(
    rhei: &Rhei,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    // §FS-rhei-plan-language.3.11: Task execution override validation.
    let declared_models: HashSet<&str> = machine.models.iter().map(String::as_str).collect();

    for_each_node(rhei, |task| {
        let subject = format!("{} {}", title_case_kind(&task.kind), task.id);
        let has_model = task.model.is_some();
        let has_target = task.target.is_some();
        if has_model && has_target {
            report.errors.push(format!(
                "{} declares both **Model:** and **Target:**; task execution overrides are mutually exclusive",
                subject
            ));
        }

        if let Some(model) = task.model.as_deref() {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                report.errors.push(format!("{} declares an empty **Model:** override", subject));
            } else if !declared_models.contains(trimmed) {
                report.errors.push(format!(
                    "{} declares **Model:** '{}' but the active state machine does not declare that model",
                    subject, trimmed
                ));
            }
        }

        if let Some(target) = task.target.as_deref() {
            let trimmed = target.trim();
            if trimmed.is_empty() {
                report.errors.push(format!("{} declares an empty **Target:** override", subject));
            } else if let Err(err) = parse_execution_target(trimmed) {
                report.errors.push(format!(
                    "{} declares invalid **Target:** '{}': {}",
                    subject, trimmed, err
                ));
            }
        }

        if !has_model && !has_target {
            return;
        }

        let parsed = parse_task_state(&task.state, machine);
        let Some(state_def) = machine.states.get(&parsed.state) else {
            return;
        };
        if !state_def.all_targets.is_empty() || !state_def.all_models.is_empty() {
            report.errors.push(format!(
                "{} declares a task execution override but state '{}' is a fanout state",
                subject, parsed.state
            ));
        }
        if state_def.target_locked {
            report.errors.push(format!(
                "{} declares a task execution override but state '{}' has target_locked: true",
                subject, parsed.state
            ));
        }
    });
}

fn title_case_kind(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len());
    let mut chars = kind.chars();
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            out.push(c);
        }
    }
    for c in chars {
        out.push(c);
    }
    out
}

/// Enforce that the authored state (ignoring any `-<visit>` suffix) is a
/// member of the resolved profile's `allowed` set. No-op when the machine
/// declares no `profiles` / `node_policy`.
fn validate_task_state_against_profile(
    subject: &str,
    raw_state: &str,
    kind: &str,
    level: u8,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    let Some(profile) = machine.profile_for_node(kind, level) else {
        return;
    };

    let parsed = parse_task_state(raw_state, machine);
    if !machine.is_valid_state(&parsed.state) {
        // `validate_task_state_instance` already reported the invalid state.
        return;
    }

    if !profile.allowed.iter().any(|s| s == &parsed.state) {
        let allowed = profile.allowed.join(", ");
        report.errors.push(format!(
            "{} has state '{}' which is not allowed by its resolved profile. Profile allows: [{}]",
            subject, parsed.state, allowed
        ));
    }
}

fn validate_task_state_instance(
    subject: &str,
    raw_state: &str,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    let parsed = parse_task_state(raw_state, machine);
    if !machine.is_valid_state(&parsed.state) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        report
            .errors
            .push(format!("{} has invalid state '{}'. Allowed: [{}]", subject, raw_state, allowed));
        return;
    }

    let Some(visit) = parsed.visit else {
        return;
    };

    if visit <= 1 {
        report.errors.push(format!(
            "{} has invalid counted state '{}'. Visit suffix '-1' is not allowed; omit the suffix for the first visit.",
            subject, raw_state
        ));
        return;
    }

    let state_def = &machine.states[&parsed.state];
    let Some(limit) = state_def.visits else {
        report.errors.push(format!(
            "{} has invalid counted state '{}'. State '{}' does not declare 'visits'.",
            subject, raw_state, parsed.state
        ));
        return;
    };

    if visit > limit {
        report.errors.push(format!(
            "{} has invalid counted state '{}'. Visit {} exceeds the declared limit {} for state '{}'.",
            subject, raw_state, visit, limit, parsed.state
        ));
    }
}

/// Warn when an authored `**Assignee:**` value is empty after trim.
///
/// The spec treats the field itself as optional; its *value* is only
/// required to be a non-empty title when present.
fn validate_assignee_nonempty(rhei: &Rhei, report: &mut ValidationReport) {
    for_each_node(rhei, |task| {
        if let Some(assignee) = &task.assignee {
            if assignee.trim().is_empty() {
                report.warnings.push(format!("Task {} has an empty **Assignee:** value", task.id));
            }
        }
    });
}

/// Verify that sibling ids are unique under the same parent and that every
/// child id extends its parent id by exactly one segment.
///
/// Together with the segment-extension check, this also implies global id
/// uniqueness across the whole plan, so no separate global pass is needed.
fn validate_sibling_uniqueness(rhei: &Rhei, report: &mut ValidationReport) {
    fn recurse(parent: Option<&Task>, siblings: &[Task], report: &mut ValidationReport) {
        let mut seen: HashSet<TaskId> = HashSet::new();
        for task in siblings {
            if let Some(p) = parent {
                if !task.id.extends(&p.id) {
                    report.errors.push(format!(
                        "Task {} must extend parent Task {} by exactly one segment",
                        task.id, p.id
                    ));
                }
            }
            if !seen.insert(task.id.clone()) {
                report.errors.push(format!(
                    "Duplicate sibling task id: Task {}{}",
                    task.id,
                    parent.map(|p| format!(" under Task {}", p.id)).unwrap_or_default()
                ));
            }
            recurse(Some(task), &task.children, report);
        }
    }
    recurse(None, &rhei.tasks, report);
}

/// Enforce that a terminal node has no non-terminal descendants anywhere in
/// its subtree.
fn validate_terminal_tree_coherence(
    rhei: &Rhei,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    fn is_terminal(state_raw: &str, machine: &StateMachine) -> bool {
        let parsed = parse_task_state(state_raw, machine);
        machine.states.get(&parsed.state).map(|d| d.terminal).unwrap_or(false)
    }

    fn check_descendants(
        ancestor: &Task,
        node: &Task,
        machine: &StateMachine,
        report: &mut ValidationReport,
    ) {
        for child in &node.children {
            if !is_terminal(&child.state, machine) {
                report.errors.push(format!(
                    "Task {} is in terminal state '{}' but descendant Task {} ('{}') is in non-terminal state '{}'",
                    ancestor.id, ancestor.state, child.id, child.title, child.state
                ));
            }
            check_descendants(ancestor, child, machine, report);
        }
    }

    for_each_node(rhei, |task| {
        if is_terminal(&task.state, machine) {
            check_descendants(task, task, machine, report);
        }
    });
}
