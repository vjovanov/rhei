//! Semantic validation for parsed Rhei markdown plans.
//!
//! This crate provides two main pieces:
//! - [`StateMachine`], loaded from YAML, which defines allowed task states
//! - validation helpers such as [`validate_with_machine`] and
//!   [`validate_from_machine_file`] that check a parsed
//!   [`rhei_core::ast::Saga`](rhei_core::ast::Saga)
//!
//! The current validator enforces the behaviors implemented in this repository:
//! dependency existence, required `**State:**` metadata, state validity,
//! `**State:**` before `**Prior:**`, circular dependency detection, and
//! subtask parent-number consistency for numeric task identifiers.

use rhei_core::ast::{Saga, Task, TaskId};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// Returns the crate version reported by Cargo metadata.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Result of validating a parsed plan.
///
/// Errors indicate invalid input. Warnings indicate accepted input with
/// noteworthy conditions, such as subtasks under named task identifiers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Validation failures that should cause command execution to fail.
    pub errors: Vec<String>,
    /// Non-fatal validation observations.
    pub warnings: Vec<String>,
}

impl ValidationReport {
    /// Construct an empty, successful report.
    pub fn ok() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Returns true if any errors are present.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Merge another report into this one.
    pub fn extend(&mut self, other: ValidationReport) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

/// Trait for validating a value into a [`ValidationReport`].
///
/// Implementations can be provided for values that do not require external
/// context. For markdown plan validation against allowed states, prefer
/// [`Validator`] or [`validate_with_machine`].
pub trait Validate {
    /// Validate `self` and collect any errors or warnings.
    fn validate(&self) -> ValidationReport;
}

/// A no-op validator implementation useful for smoke tests.
impl Validate for () {
    fn validate(&self) -> ValidationReport {
        ValidationReport::ok()
    }
}

// ===============================
// States Loader (Task 4)
// ===============================

/// Error returned when loading a [`StateMachine`] from YAML text or a file.
#[derive(Debug)]
pub enum StateMachineLoadError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
}

impl std::fmt::Display for StateMachineLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateMachineLoadError::Io(e) => write!(f, "I/O error: {e}"),
            StateMachineLoadError::Yaml(e) => write!(f, "YAML error: {e}"),
        }
    }
}

impl std::error::Error for StateMachineLoadError {}

impl From<std::io::Error> for StateMachineLoadError {
    fn from(e: std::io::Error) -> Self {
        StateMachineLoadError::Io(e)
    }
}

impl From<serde_yaml::Error> for StateMachineLoadError {
    fn from(e: serde_yaml::Error) -> Self {
        StateMachineLoadError::Yaml(e)
    }
}

/// One entry from the `states` map in a YAML states file.
#[derive(Debug, Clone, Deserialize)]
pub struct StateDef {
    /// Optional descriptive text; the current schema intentionally keeps this permissive.
    pub description: Option<String>,
}

/// States data loaded from YAML.
///
/// `version` is stored as [`serde_yaml::Value`] so the repository can accept
/// either numeric or string YAML values without imposing a stricter schema.
#[derive(Debug, Clone, Deserialize)]
pub struct StateMachine {
    /// Human-readable states definition name.
    pub name: String,
    /// YAML version field as provided by the source file.
    pub version: serde_yaml::Value,
    /// Allowed states keyed by their exact textual names.
    pub states: HashMap<String, StateDef>,
}

impl StateMachine {
    /// Load a StateMachine from YAML string contents.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, StateMachineLoadError> {
        let sm: Self = serde_yaml::from_str(yaml)?;
        Ok(sm)
    }

    /// Load a StateMachine from a file path.
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self, StateMachineLoadError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&text)
    }

    /// Returns true if `state` is among the allowed states.
    pub fn is_valid_state<S: AsRef<str>>(&self, state: S) -> bool {
        self.states.contains_key(state.as_ref())
    }

    /// Return the set of allowed state names.
    pub fn allowed_states(&self) -> impl Iterator<Item = &str> {
        self.states.keys().map(|s| s.as_str())
    }
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

    /// Validate a parsed saga using the currently configured states.
    pub fn validate(&self, saga: &Saga) -> ValidationReport {
        let mut report = ValidationReport::ok();

        let index = build_task_index(saga);
        validate_dependency_integrity(saga, &index, &mut report);
        validate_state_consistency(saga, &self.machine, &mut report);
        validate_metadata_ordering(saga, &mut report);
        validate_subtask_numbering(saga, &mut report);
        validate_circular_dependencies(saga, &index, &mut report);

        report
    }
}

/// Validate a parsed saga using an already-loaded [`StateMachine`].
pub fn validate_with_machine(saga: &Saga, machine: &StateMachine) -> ValidationReport {
    Validator::new(machine.clone()).validate(saga)
}

/// Load a [`StateMachine`] from `machine_path` and validate a parsed saga.
pub fn validate_from_machine_file<P: AsRef<Path>>(
    saga: &Saga,
    machine_path: P,
) -> Result<ValidationReport, StateMachineLoadError> {
    let machine = StateMachine::from_yaml_file(machine_path)?;
    Ok(Validator::new(machine).validate(saga))
}

// ---------------------------
// Validation helpers
// ---------------------------

fn build_task_index<'a>(saga: &'a Saga) -> HashMap<TaskId, &'a Task> {
    let mut map = HashMap::with_capacity(saga.tasks.len());
    for t in &saga.tasks {
        map.insert(t.id.clone(), t);
    }
    map
}

fn validate_dependency_integrity(
    saga: &Saga,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    for task in &saga.tasks {
        for dep in &task.metadata.depends_on {
            if !index.contains_key(dep) {
                report.errors.push(format!(
                    "Task {} depends on missing Task {}",
                    task.id, dep
                ));
            }
        }
    }
}

fn validate_state_consistency(
    saga: &Saga,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    for task in &saga.tasks {
        match task.metadata.state.as_deref() {
            None => {
                // Per spec, State is mandatory.
                report
                    .errors
                    .push(format!("Task {} is missing mandatory **State:** metadata", task.id));
            }
            Some(state) => {
                if !machine.is_valid_state(state) {
                    let allowed = machine
                        .allowed_states()
                        .collect::<Vec<_>>()
                        .join(", ");
                    report.errors.push(format!(
                        "Task {} has invalid state '{}'. Allowed: [{}]",
                        task.id, state, allowed
                    ));
                }
            }
        }
    }
}

fn validate_metadata_ordering(saga: &Saga, report: &mut ValidationReport) {
    for task in &saga.tasks {
        // Ordering only matters if Prior is present alongside State.
        if !task.metadata.depends_on.is_empty() && !task.metadata.state_first {
            report.errors.push(format!(
                "Task {} metadata order invalid: **State:** must appear before **Prior:**",
                task.id
            ));
        }
    }
}

fn validate_subtask_numbering(saga: &Saga, report: &mut ValidationReport) {
    for task in &saga.tasks {
        for st in &task.subtasks {
            match task.id {
                TaskId::Number(n) => {
                    if st.task_number != n {
                        report.errors.push(format!(
                            "Subtask {}.{} ('{}') is under Task {} but declares parent {}",
                            st.task_number, st.subtask_number, st.title, n, st.task_number
                        ));
                    }
                }
                TaskId::Named(ref name) => {
                    // Parent task has a named id; subtask numbering refers to numeric parent.
                    // Emit a warning since we cannot verify numeric consistency against a named id.
                    report.warnings.push(format!(
                        "Cannot validate subtask numbering for named task '{}'; subtasks use numeric parent identifiers",
                        name
                    ));
                }
            }
        }
    }
}

/// Detect cycles using Kahn's algorithm; report a generic cycle set on failure.
fn validate_circular_dependencies(
    _saga: &Saga,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    // Build adjacency as dep -> dependent
    let mut nodes: HashSet<TaskId> = index.keys().cloned().collect();
    let mut adj: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    let mut indegree: HashMap<TaskId, usize> = HashMap::new();

    for n in nodes.clone() {
        adj.entry(n.clone()).or_default();
        indegree.entry(n).or_insert(0);
    }

    for task in index.values() {
        // task depends on deps; edges: dep -> task.id
        for dep in &task.metadata.depends_on {
            // Include unseen dependency as a node to make cycle detection robust even if integrity check was skipped.
            nodes.insert(dep.clone());
            adj.entry(dep.clone()).or_default().push(task.id.clone());
            *indegree.entry(task.id.clone()).or_insert(0) += 1;
            indegree.entry(dep.clone()).or_insert(0);
        }
    }

    // Kahn's algorithm
    let mut q: VecDeque<TaskId> = indegree
        .iter()
        .filter_map(|(n, &d)| if d == 0 { Some(n.clone()) } else { None })
        .collect();
    let mut processed = 0usize;

    while let Some(n) = q.pop_front() {
        processed += 1;
        if let Some(neigh) = adj.get(&n) {
            for m in neigh {
                if let Some(d) = indegree.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        q.push_back(m.clone());
                    }
                }
            }
        }
    }

    if processed != indegree.len() {
        // Collect nodes still with indegree > 0
        let cyc_nodes: Vec<String> = indegree
            .iter()
            .filter_map(|(n, &d)| if d > 0 { Some(n.to_string()) } else { None })
            .collect();
        if !cyc_nodes.is_empty() {
            report.errors.push(format!(
                "Circular dependency detected among tasks: {:?}",
                cyc_nodes
            ));
        } else {
            report
                .errors
                .push("Circular dependency detected (unable to isolate nodes)".to_string());
        }
    }
}

// ---------------------------
// Tests
// ---------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rhei_core::parse;

    fn sample_machine() -> StateMachine {
        let yaml = r#"
name: test-sm
version: 1.0
states:
  pending: { description: "not started" }
  in-progress: { description: "doing" }
  completed: { description: "done" }
"#;
        StateMachine::from_yaml_str(yaml).expect("states load")
    }

    #[test]
    fn detects_missing_state_and_bad_dependency_and_cycle() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 3

#### Subtask 1.1: s

### Task 2: B
**State:** invalid_state

### Task 3: C
**State:** pending
**Prior:** Task 1
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors());
        // Expect: Task 1 depends on 3 (exists), Task 3 depends on 1 => cycle
        // Also: Task 2 has invalid state
        let joined = report.errors.join("\n");
        assert!(joined.contains("invalid state"));
        assert!(joined.contains("Circular dependency detected"));
    }

    #[test]
    fn ok_when_valid() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** pending

#### Subtask 1.1: s

### Task 2: B
**State:** in-progress
**Prior:** Task 1
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn reports_missing_numeric_dependency() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 9

### Task 2: B
**State:** in-progress
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected missing dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 1 depends on missing Task 9"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn reports_missing_named_dependency() {
        let input = r#"# Saga: Example
## Tasks

### Task build: Build step
**State:** pending
**Prior:** Task deploy

### Task test: Test step
**State:** in-progress
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected missing named dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task build depends on missing Task deploy"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn ok_when_all_dependencies_exist_named_and_numeric() {
        let input = r#"# Saga: Example
## Tasks

### Task init: Initialize
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task init

### Task 1: A
**State:** completed
**Prior:** Task 2, Task init
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(
            !report.has_errors(),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn reports_missing_state_is_error() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected missing state error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("missing mandatory **State:**"),
            "did not find missing state message; got:\n{}",
            joined
        );
    }

    #[test]
    fn reports_invalid_state_with_allowed_list() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** invalid_state
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected invalid state error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("invalid state"),
            "did not find 'invalid state' in errors:\n{}",
            joined
        );
        assert!(
            joined.contains("Allowed: ["),
            "did not include 'Allowed: [...]' list:\n{}",
            joined
        );
        for s in ["pending", "in-progress", "completed"] {
            assert!(
                joined.contains(s),
                "allowed list missing state '{}'; errors:\n{}",
                s,
                joined
            );
        }
    }

    #[test]
    fn accepts_valid_states_and_escaped_spaces() {
        // Custom states definition with a state containing a space
        let yaml = r#"
name: sm-escaped
version: 1
states:
  "in progress": { description: "with space" }
"#;
        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** in\ progress
"#;
        let saga = parse(input).expect("parse ok");
        let report = validate_with_machine(&saga, &machine);

        assert!(
            !report.has_errors(),
            "unexpected errors validating escaped-space state: {:?}",
            report.errors
        );
    }

    #[test]
    fn ok_when_all_tasks_have_valid_state() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress

### Task 3: C
**State:** completed
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(
            !report.has_errors(),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn detects_two_node_cycle() {
        let input = r#"# Saga: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** pending
**Prior:** Task 1
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
        assert!(joined.contains("2"), "should mention task 2; got:\n{}", joined);
    }

    #[test]
    fn detects_three_node_cycle() {
        let input = r#"# Saga: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** in-progress
**Prior:** Task 3

### Task 3: C
**State:** completed
**Prior:** Task 1
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        // At least two task ids should be mentioned; typically all three.
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
        assert!(joined.contains("2"), "should mention task 2; got:\n{}", joined);
    }

    #[test]
    fn detects_self_cycle() {
        let input = r#"# Saga: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 1
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected self-cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
    }

    #[test]
    fn passes_on_dag() {
        let input = r#"# Saga: Ex
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task 1

### Task 3: C
**State:** completed
**Prior:** Task 2
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(
            !report.has_errors(),
            "unexpected errors in DAG case: {:?}",
            report.errors
        );
    }

    #[test]
    fn no_false_cycle_with_missing_dependency() {
        let input = r#"# Saga: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 9

### Task 2: B
**State:** in-progress
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected missing dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 1 depends on missing Task 9"),
            "did not find expected missing-dep message; got:\n{}",
            joined
        );
        assert!(
            !joined.contains("Circular dependency detected"),
            "should not report a cycle when only a dependency is missing; got:\n{}",
            joined
        );
    }

    // ---- Task 5.4: Subtask numbering validation tests ----

    #[test]
    fn mismatched_parent_number_errors() {
        let input = r#"# Saga: Example
## Tasks

### Task 2: B
**State:** pending

#### Subtask 1.1: Wrong parent number
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected numbering mismatch error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Subtask 1.1"),
            "error should mention subtask 1.1; got:\n{}",
            joined
        );
        assert!(
            joined.contains("under Task 2"),
            "error should mention it is under Task 2; got:\n{}",
            joined
        );
    }

    #[test]
    fn valid_subtask_numbering_ok() {
        let input = r#"# Saga: Example
## Tasks

### Task 3: C
**State:** pending

#### Subtask 3.1: First
#### Subtask 3.2: Second
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(
            !report.has_errors(),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn named_task_subtasks_warn_only() {
        let input = r#"# Saga: Example
## Tasks

### Task build: B
**State:** pending

#### Subtask 1.1: Any number
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(
            !report.has_errors(),
            "named task with subtasks should not produce errors; got: {:?}",
            report.errors
        );
        let warnings = report.warnings.join("\n");
        assert!(
            warnings.contains("Cannot validate subtask numbering for named task 'build'"),
            "expected named-task numbering warning; warnings were:\n{}",
            warnings
        );
    }

    #[test]
    fn mixed_tasks_ok_and_error() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**State:** pending

#### Subtask 1.1: Correct

### Task 2: B
**State:** pending

#### Subtask 1.2: Incorrect parent
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let mut report = validate_with_machine(&saga, &sm);

        assert!(
            report.has_errors(),
            "expected exactly one numbering error; got none"
        );
        // Filter only numbering mismatch errors to be robust to future validators.
        report.errors.retain(|e| e.contains("Subtask 1.2") && e.contains("under Task 2"));
        assert_eq!(
            report.errors.len(),
            1,
            "expected exactly one numbering mismatch error; got: {:?}",
            report.errors
        );
        let e = &report.errors[0];
        assert!(e.contains("Subtask 1.2"), "error should mention Subtask 1.2; got:\n{}", e);
        assert!(e.contains("under Task 2"), "error should mention under Task 2; got:\n{}", e);
    }

    #[test]
    fn multiple_subtasks_some_bad() {
        let input = r#"# Saga: Example
## Tasks

### Task 4: D
**State:** pending

#### Subtask 4.1: Correct
#### Subtask 3.2: Incorrect parent
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected at least one numbering error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Subtask 3.2") && joined.contains("under Task 4"),
            "expected mismatch message for 3.2 under Task 4; got:\n{}",
            joined
        );
    }

    #[test]
    fn reports_metadata_ordering_when_prior_precedes_state() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**Prior:** Task 2
**State:** pending

### Task 2: B
**State:** completed
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        assert!(report.has_errors(), "expected metadata ordering error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 1 metadata order invalid"),
            "expected metadata ordering message; got:\n{}",
            joined
        );
        assert!(
            joined.contains("**State:** must appear before **Prior:**"),
            "expected ordering rule details; got:\n{}",
            joined
        );
    }

    #[test]
    fn prior_without_state_reports_missing_state_and_ordering_error() {
        let input = r#"# Saga: Example
## Tasks

### Task 1: A
**Prior:** Task 2

### Task 2: B
**State:** pending
"#;
        let saga = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&saga, &sm);

        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 1 is missing mandatory **State:** metadata"),
            "expected missing-state error; got:\n{}",
            joined
        );
        assert!(
            joined.contains("Task 1 metadata order invalid"),
            "expected metadata ordering error; got:\n{}",
            joined
        );
    }

    #[test]
    fn validation_report_extend_merges_errors_and_warnings() {
        let mut base = ValidationReport {
            errors: vec!["e1".to_string()],
            warnings: vec!["w1".to_string()],
        };
        let other = ValidationReport {
            errors: vec!["e2".to_string()],
            warnings: vec!["w2".to_string()],
        };

        base.extend(other);

        assert_eq!(base.errors, vec!["e1".to_string(), "e2".to_string()]);
        assert_eq!(base.warnings, vec!["w1".to_string(), "w2".to_string()]);
    }

    #[test]
    fn unit_type_validate_returns_ok_report() {
        let report = ().validate();

        assert_eq!(report, ValidationReport::ok());
        assert!(!report.has_errors());
    }
}
