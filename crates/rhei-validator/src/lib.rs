//! Semantic validation for parsed Rhei markdown plans.
//!
//! This crate provides two main pieces:
//! - [`StateMachine`], loaded from YAML, which defines allowed task states
//! - validation helpers such as [`validate_with_machine`] and
//!   [`validate_from_machine_file`] that check a parsed
//!   [`rhei_core::ast::Rhei`](rhei_core::ast::Rhei)
//!
//! The current validator enforces the behaviors implemented in this repository:
//! dependency existence, required `**State:**` metadata, state validity,
//! `**State:**` before `**Prior:**`, circular dependency detection,
//! subtask parent-number consistency for numeric task identifiers, and
//! terminal parent/subtask coherence.

use indexmap::IndexMap;
use regex::Regex;
pub use rhei_core::ast::{CallbackRef, StateName, TransitionRule};
use rhei_core::ast::{Rhei, Task, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Component, Path};

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
        Self { errors: Vec::new(), warnings: Vec::new() }
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
    Invalid(String),
}

impl std::fmt::Display for StateMachineLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateMachineLoadError::Io(e) => write!(f, "I/O error: {e}"),
            StateMachineLoadError::Yaml(e) => write!(f, "YAML error: {e}"),
            StateMachineLoadError::Invalid(message) => {
                write!(f, "invalid state machine: {message}")
            }
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateArtifactDef {
    /// Stable identifier for the artifact within a state.
    pub name: String,
    /// Workspace-relative artifact path template.
    pub path: String,
    /// Optional human-readable description of the artifact.
    #[serde(default)]
    pub description: Option<String>,
    /// When `true`, a missing file does not block state entry.
    /// Only valid on `inputs` entries; declaring `optional: true` on an
    /// `outputs` entry is a validation error.
    #[serde(default)]
    pub optional: bool,
}

/// Agent configuration: either a known agent ID string or a custom profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AgentConfig {
    /// Known agent identifier (e.g., `"claude-code"`, `"codex"`).
    Known(String),
    /// Custom agent profile with command and flags.
    Custom(CustomAgentProfile),
}

impl AgentConfig {
    /// Return the agent identifier regardless of variant.
    pub fn id(&self) -> &str {
        match self {
            AgentConfig::Known(id) => id,
            AgentConfig::Custom(profile) => &profile.id,
        }
    }
}

/// Custom agent profile for agents not in the built-in list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CustomAgentProfile {
    /// Identifier for logs and diagnostics.
    pub id: String,
    /// Base command and fixed arguments.
    pub command: Vec<String>,
    /// Flag to pass the prompt (e.g., `"--prompt"`, `"-p"`). Omit if using stdin.
    #[serde(default)]
    pub prompt_flag: Option<String>,
    /// Flag to pass the model. Omit if the agent doesn't support model selection.
    #[serde(default)]
    pub model_flag: Option<String>,
    /// When `true`, the prompt is piped to stdin instead of passed via flag.
    #[serde(default)]
    pub stdin_prompt: bool,
    /// Default timeout for this agent (e.g., `"30m"`).
    #[serde(default)]
    pub timeout: Option<String>,
}

/// One entry from the `states` map in a YAML states file.
#[derive(Debug, Clone, Deserialize)]
pub struct StateDef {
    /// Optional descriptive text; the current schema intentionally keeps this permissive.
    pub description: Option<String>,
    /// Optional agent-facing instructions for what to do while a task is in this state.
    #[serde(default)]
    pub instructions: Option<String>,
    /// Optional persona/instructions that frame how an agent approaches tasks in this state.
    #[serde(default)]
    pub personality: Option<String>,
    /// Marks this state as the initial state in the state machine.
    #[serde(default)]
    pub initial: bool,
    /// Marks this state as a final/terminal state in the state machine.
    #[serde(default, rename = "final")]
    pub terminal: bool,
    /// When `true`, autonomous commands must not transition out of this state.
    #[serde(default)]
    pub gating: bool,
    /// Optional visit budget for returning to this state.
    pub visits: Option<u32>,
    /// Explicit list of declared models that should each execute this state.
    #[serde(default)]
    pub all_models: Vec<String>,
    /// Restricts this state to one declared model.
    #[serde(default)]
    pub model: Option<String>,
    /// The coding agent CLI that executes work in this state.
    #[serde(default)]
    pub agent: Option<AgentConfig>,
    /// Maximum time an agent may work in this state (e.g., `"30m"`, `"1h"`).
    #[serde(default)]
    pub agent_timeout: Option<String>,
    /// Deterministic program command for this state (mutually exclusive with `agent`).
    #[serde(default)]
    pub program: Option<serde_yaml::Value>,
    /// Maximum time the program may run in this state (e.g., `"10m"`, `"1h"`).
    #[serde(default)]
    pub program_timeout: Option<String>,
    /// Required artifacts that must exist before work can proceed in this state.
    #[serde(default)]
    pub inputs: Vec<StateArtifactDef>,
    /// Required artifacts that must exist before leaving this state.
    #[serde(default)]
    pub outputs: Vec<StateArtifactDef>,
}

/// States data loaded from YAML.
///
/// `version` is stored as [`serde_yaml::Value`] so the repository can accept
/// either numeric or string YAML values without imposing a stricter schema.
#[derive(Debug, Clone, Deserialize)]
pub struct StateMachine {
    /// Human-readable states definition name.
    pub name: String,
    /// Optional declared model identifiers available to states in this machine.
    #[serde(default)]
    pub models: Vec<String>,
    /// YAML version field as provided by the source file.
    pub version: serde_yaml::Value,
    /// Allowed states keyed by their exact textual names, preserving YAML order.
    pub states: IndexMap<String, StateDef>,
    /// Declared allowed transitions between states. Empty if unspecified.
    #[serde(default)]
    pub transitions: Vec<TransitionRule>,
}

/// The built-in default states YAML shipped with rhei.
const DEFAULT_STATES_YAML: &str = include_str!("default-states.yaml");

impl StateMachine {
    /// Return the built-in default state machine shipped with rhei.
    pub fn builtin_default() -> Self {
        Self::from_yaml_str(DEFAULT_STATES_YAML).expect("built-in states YAML is always valid")
    }

    /// Load a StateMachine from YAML string contents.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, StateMachineLoadError> {
        let sm: Self = serde_yaml::from_str(yaml)?;
        sm.validate_model_configuration()?;
        sm.validate_program_configuration()?;
        sm.validate_template_conditions()?;
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

    /// Return the declared transitions between states.
    pub fn transitions(&self) -> &[TransitionRule] {
        &self.transitions
    }

    fn validate_model_configuration(&self) -> Result<(), StateMachineLoadError> {
        let mut seen = HashSet::new();
        for model in &self.models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "top-level 'models' entries must be non-empty strings".to_string(),
                ));
            }
            if !seen.insert(trimmed) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "top-level 'models' contains duplicate entry '{trimmed}'"
                )));
            }
        }

        for (state_name, state) in &self.states {
            if !state.all_models.is_empty() && state.model.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot set both 'all_models' and 'model'"
                )));
            }

            if state.visits == Some(0) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'visits: 0' but visits must be at least 1"
                )));
            }

            validate_artifact_definitions(state_name, "inputs", &state.inputs)?;
            validate_artifact_definitions(state_name, "outputs", &state.outputs)?;

            // Agent validation.
            if let Some(agent) = &state.agent {
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare an 'agent' (terminal states have no work to execute)"
                    )));
                }
                match agent {
                    AgentConfig::Known(id) => {
                        if id.trim().is_empty() {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' declares an empty 'agent' value"
                            )));
                        }
                    }
                    AgentConfig::Custom(profile) => {
                        if profile.id.trim().is_empty() {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' custom agent profile has an empty 'id'"
                            )));
                        }
                        if profile.command.is_empty() {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' custom agent profile has an empty 'command'"
                            )));
                        }
                    }
                }
            }
            if let Some(timeout) = &state.agent_timeout {
                if parse_duration_secs(timeout).is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'agent_timeout' value '{timeout}' \
                         (expected format like '30s', '5m', '1h', '2h30m')"
                    )));
                }
            }
            if let Some(timeout) = &state.program_timeout {
                if parse_duration_secs(timeout).is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'program_timeout' value '{timeout}' \
                         (expected format like '30s', '5m', '1h', '2h30m')"
                    )));
                }
            }
            if state.agent.is_some() && state.program.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot declare both 'agent' and 'program' (they are mutually exclusive)"
                )));
            }

            if !state.all_models.is_empty() && self.models.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' sets 'all_models' but the machine does not declare any top-level 'models'"
                )));
            }

            let mut state_seen = HashSet::new();
            for model in &state.all_models {
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' contains an empty 'all_models' entry"
                    )));
                }
                if !state_seen.insert(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' contains duplicate 'all_models' entry '{trimmed}'"
                    )));
                }
                if !seen.contains(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' references unknown model '{trimmed}' in 'all_models'"
                    )));
                }
            }

            if let Some(model) = state.model.as_deref() {
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'model' value"
                    )));
                }
                if self.models.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' sets 'model: {trimmed}' but the machine does not declare any top-level 'models'"
                    )));
                }
                if !seen.contains(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' references unknown model '{trimmed}'"
                    )));
                }
            }
        }

        Ok(())
    }

    fn validate_program_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            if let Some(program) = &state.program {
                validate_program_value(state_name, program)?;
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare a 'program' (terminal states have no work to execute)"
                    )));
                }
                if state.gating {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is gating and cannot declare a 'program' (gating states require human action)"
                    )));
                }
            }
        }

        for transition in &self.transitions {
            if transition.exit_code.is_none() {
                continue;
            }

            let Some(from_state) = self.states.get(&transition.from.0) else {
                continue;
            };
            if from_state.program.is_none() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "transition from '{}' to '{}' declares 'exit_code' but source state '{}' does not declare a program",
                    transition.from.0, transition.to.0, transition.from.0
                )));
            }
        }

        Ok(())
    }

    /// Validate that every `{if <condition>}` tag in `instructions` and
    /// `personality` fields references a condition the engine can evaluate.
    ///
    /// Currently the only supported condition form is `input.<name>.exists`,
    /// where `<name>` must match a declared input artifact on the same state.
    /// Any other condition is a load-time error.
    fn validate_template_conditions(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            for (field_name, text) in [
                ("instructions", state.instructions.as_deref()),
                ("personality", state.personality.as_deref()),
            ] {
                let Some(text) = text else { continue };
                for condition in extract_if_conditions(text) {
                    if let Some(input_name) =
                        condition.strip_prefix("input.").and_then(|s| s.strip_suffix(".exists"))
                    {
                        if !state.inputs.iter().any(|a| a.name == input_name) {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' {field_name} contains \
                                 '{{if {condition}}}' but '{input_name}' is not a declared input \
                                 on this state"
                            )));
                        }
                    } else {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' {field_name} contains \
                             '{{if {condition}}}' which is not a recognised condition; \
                             supported form: 'input.<name>.exists'"
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

/// Extract every condition string from `{if <condition>}` tags in `text`.
fn extract_if_conditions(text: &str) -> Vec<&str> {
    let mut conditions = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("{if ") {
        let after_open = start + "{if ".len();
        if let Some(close) = remaining[after_open..].find('}') {
            conditions.push(&remaining[after_open..after_open + close]);
            remaining = &remaining[after_open + close + 1..];
        } else {
            break;
        }
    }
    conditions
}

fn validate_program_value(
    state_name: &str,
    value: &serde_yaml::Value,
) -> Result<(), StateMachineLoadError> {
    match value {
        serde_yaml::Value::String(command) => {
            if command.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares an empty 'program' value"
                )));
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let Some(command) = mapping.get(serde_yaml::Value::String("command".to_string()))
            else {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' program object must include a 'command' field"
                )));
            };
            validate_program_command(state_name, command)?;

            if let Some(env) = mapping.get(serde_yaml::Value::String("env".to_string())) {
                match env {
                    serde_yaml::Value::Mapping(env_map) => {
                        for (key, value) in env_map {
                            let Some(key) = key.as_str() else {
                                return Err(StateMachineLoadError::Invalid(format!(
                                    "state '{state_name}' program.env keys must be strings"
                                )));
                            };
                            if key.trim().is_empty() {
                                return Err(StateMachineLoadError::Invalid(format!(
                                    "state '{state_name}' program.env contains an empty key"
                                )));
                            }
                            if !matches!(
                                value,
                                serde_yaml::Value::Null
                                    | serde_yaml::Value::Bool(_)
                                    | serde_yaml::Value::Number(_)
                                    | serde_yaml::Value::String(_)
                            ) {
                                return Err(StateMachineLoadError::Invalid(format!(
                                    "state '{state_name}' program.env['{key}'] must be a scalar value"
                                )));
                            }
                        }
                    }
                    _ => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.env must be a mapping"
                        )))
                    }
                }
            }

            if let Some(working_directory) =
                mapping.get(serde_yaml::Value::String("working_directory".to_string()))
            {
                match working_directory {
                    serde_yaml::Value::String(path) if !path.trim().is_empty() => {}
                    serde_yaml::Value::String(_) => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.working_directory must be a non-empty string"
                        )))
                    }
                    _ => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.working_directory must be a string"
                        )))
                    }
                }
            }

            if let Some(shell) = mapping.get(serde_yaml::Value::String("shell".to_string())) {
                if !matches!(shell, serde_yaml::Value::Bool(_)) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' program.shell must be a boolean"
                    )));
                }
            }
        }
        _ => {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' program must be a non-empty string or an object"
            )))
        }
    }

    Ok(())
}

fn validate_program_command(
    state_name: &str,
    command: &serde_yaml::Value,
) -> Result<(), StateMachineLoadError> {
    match command {
        serde_yaml::Value::String(value) => {
            if value.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' program.command must be a non-empty string"
                )));
            }
        }
        serde_yaml::Value::Sequence(values) => {
            if values.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' program.command array must not be empty"
                )));
            }
            for value in values {
                match value {
                    serde_yaml::Value::String(item) if !item.trim().is_empty() => {}
                    _ => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.command entries must be non-empty strings"
                        )))
                    }
                }
            }
        }
        _ => {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' program.command must be a string or string array"
            )))
        }
    }

    Ok(())
}

fn validate_artifact_definitions(
    state_name: &str,
    field_name: &str,
    artifacts: &[StateArtifactDef],
) -> Result<(), StateMachineLoadError> {
    let mut seen_names = HashSet::new();

    for artifact in artifacts {
        let name = artifact.name.trim();
        if name.is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' contains an artifact in '{field_name}' with an empty 'name'"
            )));
        }
        if !seen_names.insert(name) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' contains duplicate artifact name '{name}' in '{field_name}'"
            )));
        }

        let path = artifact.path.trim();
        if path.is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in '{field_name}' has an empty 'path'"
            )));
        }
        if artifact.optional && field_name == "outputs" {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in 'outputs' may not be marked 'optional'; only inputs may be optional"
            )));
        }
        if Path::new(path).is_absolute() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in '{field_name}' must use a relative path, got '{path}'"
            )));
        }
        if path_escapes_workspace_root(path) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in '{field_name}' escapes the workspace root via path '{path}'"
            )));
        }
    }

    Ok(())
}

fn path_escapes_workspace_root(path: &str) -> bool {
    let expanded = path.replace("{task_id}", "task").replace("{state}", "state");
    let mut depth = 0usize;

    for component in Path::new(&expanded).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return true,
            Component::ParentDir => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
        }
    }

    false
}

/// Parse a human-readable duration string into seconds.
///
/// Supported formats: `30s`, `5m`, `1h`, `2h30m`, `1h15m30s`.
/// Returns `None` if the string is not a valid duration.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let mut total: u64 = 0;
    let mut current_num = String::new();
    let mut found_any = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else {
            let n: u64 = current_num.parse().ok()?;
            current_num.clear();
            match ch {
                'h' => total = total.checked_add(n.checked_mul(3600)?)?,
                'm' => total = total.checked_add(n.checked_mul(60)?)?,
                's' => total = total.checked_add(n)?,
                _ => return None,
            }
            found_any = true;
        }
    }

    // Reject trailing digits without a unit suffix or empty input.
    if !current_num.is_empty() || !found_any {
        return None;
    }

    Some(total)
}

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
        validate_task_id_uniqueness(rhei, &mut report);
        validate_dependency_integrity(rhei, &index, &mut report);
        validate_state_consistency(rhei, &self.machine, &mut report);
        validate_subtask_state_consistency(rhei, &self.machine, &mut report);
        validate_terminal_parent_subtask_coherence(rhei, &self.machine, &mut report);
        validate_subtask_numbering(rhei, &mut report);
        validate_subtask_uniqueness(rhei, &mut report);
        validate_circular_dependencies(rhei, &index, &mut report);

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

/// Load a [`StateMachine`] from `machine_path` and validate a parsed rhei.
pub fn validate_from_machine_file<P: AsRef<Path>>(
    rhei: &Rhei,
    machine_path: P,
) -> Result<ValidationReport, StateMachineLoadError> {
    let machine = StateMachine::from_yaml_file(machine_path)?;
    Ok(Validator::new(machine).validate(rhei))
}

// ---------------------------
// Validation helpers
// ---------------------------

fn build_task_index(rhei: &Rhei) -> HashMap<TaskId, &Task> {
    let mut map = HashMap::with_capacity(rhei.tasks.len());
    for t in &rhei.tasks {
        map.insert(t.id.clone(), t);
    }
    map
}

fn validate_dependency_integrity(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    for task in &rhei.tasks {
        for dep in &task.prior {
            if !index.contains_key(dep) {
                report.errors.push(format!("Task {} depends on missing Task {}", task.id, dep));
            }
        }
    }
}

fn validate_state_consistency(rhei: &Rhei, machine: &StateMachine, report: &mut ValidationReport) {
    for task in &rhei.tasks {
        validate_task_state_instance(&format!("Task {}", task.id), &task.state, machine, report);
    }
}

fn validate_subtask_state_consistency(
    rhei: &Rhei,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    for task in &rhei.tasks {
        for st in &task.subtasks {
            validate_task_state_instance(
                &format!("Subtask {}.{} ('{}')", st.task_number, st.subtask_number, st.title),
                &st.state,
                machine,
                report,
            );
        }
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

fn validate_subtask_numbering(rhei: &Rhei, report: &mut ValidationReport) {
    for task in &rhei.tasks {
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
                    // Per spec, named tasks must not have subtasks.
                    report.errors.push(format!(
                        "Task '{}' has a named id and must not declare subtasks",
                        name
                    ));
                    break; // One error per task is sufficient
                }
            }
        }
    }
}

fn validate_task_id_uniqueness(rhei: &Rhei, report: &mut ValidationReport) {
    let mut seen = HashSet::new();
    for task in &rhei.tasks {
        if !seen.insert(task.id.clone()) {
            report.errors.push(format!("Duplicate task id: Task {}", task.id));
        }
    }
}

fn validate_subtask_uniqueness(rhei: &Rhei, report: &mut ValidationReport) {
    for task in &rhei.tasks {
        let mut seen = HashSet::new();
        for st in &task.subtasks {
            if !seen.insert(st.subtask_number) {
                report.errors.push(format!(
                    "Duplicate subtask number {}.{} under Task {}",
                    st.task_number, st.subtask_number, task.id
                ));
            }
        }
    }
}

fn validate_terminal_parent_subtask_coherence(
    rhei: &Rhei,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    for task in &rhei.tasks {
        let parent_state = parse_task_state(&task.state, machine);
        let Some(parent_def) = machine.states.get(&parent_state.state) else {
            continue;
        };
        if !parent_def.terminal {
            continue;
        }

        for st in &task.subtasks {
            let subtask_state = parse_task_state(&st.state, machine);
            let Some(subtask_def) = machine.states.get(&subtask_state.state) else {
                continue;
            };
            if subtask_def.terminal {
                continue;
            }

            report.errors.push(format!(
                "Task {} is in terminal state '{}' but Subtask {}.{} ('{}') is in non-terminal state '{}'",
                task.id,
                task.state,
                st.task_number,
                st.subtask_number,
                st.title,
                st.state
            ));
        }
    }
}

/// Extract markdown links from a text block, returning `(display_text, target)` pairs.
fn extract_markdown_links(text: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
    re.captures_iter(text).map(|cap| (cap[1].to_string(), cap[2].to_string())).collect()
}

/// Collect all markdown links from every content field in the plan.
///
/// Returns `(location_label, display_text, target)` triples.
fn collect_all_links(rhei: &Rhei) -> Vec<(String, String, String)> {
    let mut links = Vec::new();

    for section in &rhei.content_sections {
        for (display, target) in extract_markdown_links(&section.content) {
            links.push((format!("section '{}'", section.title), display, target));
        }
    }

    for task in &rhei.tasks {
        for (display, target) in extract_markdown_links(&task.content) {
            links.push((format!("Task {}", task.id), display, target));
        }
        for st in &task.subtasks {
            for (display, target) in extract_markdown_links(&st.content) {
                links.push((
                    format!("Subtask {}.{}", st.task_number, st.subtask_number),
                    display,
                    target,
                ));
            }
        }
    }

    links
}

/// Returns true if the link target looks like an external URL or a fragment-only anchor.
fn is_non_file_link(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
}

/// Validate that relative markdown links in all content fields point to
/// existing files, resolved against `base_path`.
fn validate_markdown_links(rhei: &Rhei, base_path: &Path, report: &mut ValidationReport) {
    let links = collect_all_links(rhei);

    for (location, display, target) in &links {
        if is_non_file_link(target) {
            continue;
        }

        // Strip fragment (e.g. "file.md#section" → "file.md")
        let file_part = target.split('#').next().unwrap_or(target);
        if file_part.is_empty() {
            continue; // pure fragment link, already handled above
        }

        let resolved = base_path.join(file_part);
        if !resolved.exists() {
            report.errors.push(format!(
                "{} contains a link [{}]({}) but '{}' does not exist",
                location, display, target, file_part
            ));
        }
    }
}

/// Detect cycles using Kahn's algorithm; report a generic cycle set on failure.
fn validate_circular_dependencies(
    _rhei: &Rhei,
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
        for dep in &task.prior {
            // Include unseen dependency as a node to make cycle detection robust even if integrity check was skipped.
            nodes.insert(dep.clone());
            adj.entry(dep.clone()).or_default().push(task.id.clone());
            *indegree.entry(task.id.clone()).or_insert(0) += 1;
            indegree.entry(dep.clone()).or_insert(0);
        }
    }

    // Kahn's algorithm
    let mut q: VecDeque<TaskId> =
        indegree.iter().filter_map(|(n, &d)| if d == 0 { Some(n.clone()) } else { None }).collect();
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
            report
                .errors
                .push(format!("Circular dependency detected among tasks: {:?}", cyc_nodes));
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
    use std::fs;

    fn sample_machine() -> StateMachine {
        let yaml = r#"
name: test-sm
version: 1.0
states:
  pending: { description: "not started" }
  in-progress: { description: "doing" }
  completed: { description: "done", final: true }
"#;
        StateMachine::from_yaml_str(yaml).expect("states load")
    }

    #[test]
    fn loads_state_machine_with_models_and_state_selectors() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
  - claude-sonnet
states:
  draft:
    description: planned
    visits: 2
    all_models:
      - gpt-5
      - claude-sonnet
  review:
    description: reviewed
    model: claude-sonnet
"#;

        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        assert_eq!(machine.models, vec!["gpt-5", "claude-sonnet"]);
        assert_eq!(machine.states["draft"].visits, Some(2));
        assert_eq!(machine.states["draft"].all_models, vec!["gpt-5", "claude-sonnet"]);
        assert_eq!(machine.states["review"].model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn rejects_state_machine_with_unknown_state_model() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
states:
  draft:
    description: planned
    model: claude-sonnet
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject unknown model");
        assert!(err.to_string().contains("references unknown model 'claude-sonnet'"));
    }

    #[test]
    fn rejects_state_machine_with_conflicting_state_model_selectors() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
states:
  draft:
    description: planned
    all_models:
      - gpt-5
    model: gpt-5
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject conflicting selectors");
        assert!(err.to_string().contains("cannot set both 'all_models' and 'model'"));
    }

    #[test]
    fn rejects_state_machine_with_unknown_all_models_entry() {
        let yaml = r#"
name: multi-model
version: 1.0
models:
  - gpt-5
states:
  draft:
    description: planned
    all_models:
      - claude-sonnet
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject unknown all_models entry");
        assert!(err
            .to_string()
            .contains("references unknown model 'claude-sonnet' in 'all_models'"));
    }

    #[test]
    fn rejects_state_machine_with_zero_visits() {
        let yaml = r#"
name: multi-model
version: 1.0
states:
  draft:
    description: planned
    visits: 0
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject zero visits");
        assert!(err.to_string().contains("visits: 0"));
    }

    #[test]
    fn loads_state_machine_with_artifact_contracts() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    inputs:
      - name: implementation
        path: runtime/results/{task_id}.md
        format: markdown
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
        description: Review findings
"#;

        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        assert_eq!(machine.states["review"].inputs.len(), 1);
        assert_eq!(machine.states["review"].outputs.len(), 1);
        assert_eq!(machine.states["review"].outputs[0].name, "findings");
    }

    #[test]
    fn rejects_duplicate_artifact_names_in_same_state_field() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    outputs:
      - name: findings
        path: runtime/findings/a.md
      - name: findings
        path: runtime/findings/b.md
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject duplicate names");
        assert!(err.to_string().contains("duplicate artifact name 'findings'"));
    }

    #[test]
    fn rejects_absolute_artifact_paths() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    outputs:
      - name: findings
        path: /tmp/findings.md
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject absolute path");
        assert!(err.to_string().contains("must use a relative path"));
    }

    #[test]
    fn rejects_if_condition_referencing_undeclared_input() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if input.typo.exists}
      Read the notes.
      {endif}
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
"#;
        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject unknown input reference");
        assert!(err.to_string().contains("'typo' is not a declared input"));
    }

    #[test]
    fn rejects_if_condition_with_unsupported_form() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if meta.flag}
      Extra instructions.
      {endif}
"#;
        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject unsupported condition");
        assert!(err.to_string().contains("not a recognised condition"));
    }

    #[test]
    fn accepts_if_condition_referencing_declared_input() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    instructions: |
      {if input.notes.exists}
      Read the notes.
      {endif}
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
"#;
        StateMachine::from_yaml_str(yaml).expect("valid condition should load");
    }

    #[test]
    fn accepts_if_condition_in_personality_referencing_declared_input() {
        let yaml = r#"
name: test
version: 1.0
states:
  implement:
    description: do work
    personality: |
      {if input.context.exists}
      Use context from {input.context.path}.
      {endif}
    inputs:
      - name: context
        path: runtime/context/{task_id}.md
        optional: true
"#;
        StateMachine::from_yaml_str(yaml).expect("valid condition in personality should load");
    }

    #[test]
    fn rejects_artifact_paths_that_escape_workspace_root() {
        let yaml = r#"
name: artifacts
version: 1.0
states:
  review:
    description: review work
    outputs:
      - name: findings
        path: ../../outside.md
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject escaping path");
        assert!(err.to_string().contains("escapes the workspace root"));
    }

    #[test]
    fn detects_missing_state_and_bad_dependency_and_cycle() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 3

#### Subtask 1.1: s
**State:** pending

### Task 2: B
**State:** invalid_state

### Task 3: C
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors());
        // Expect: Task 1 depends on 3 (exists), Task 3 depends on 1 => cycle
        // Also: Task 2 has invalid state
        let joined = report.errors.join("\n");
        assert!(joined.contains("invalid state"));
        assert!(joined.contains("Circular dependency detected"));
    }

    #[test]
    fn ok_when_valid() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

#### Subtask 1.1: s
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn accepts_counted_state_suffix_within_budget() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending-2
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = StateMachine::from_yaml_str(
            r#"
name: example
version: 1.0
states:
  pending:
    description: queued
    visits: 3
"#,
        )
        .expect("states load");

        let report = validate_with_machine(&rhei, &machine);
        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn rejects_counted_state_suffix_of_one() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending-1
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = StateMachine::from_yaml_str(
            r#"
name: example
version: 1.0
states:
  pending:
    description: queued
    visits: 3
"#,
        )
        .expect("states load");

        let report = validate_with_machine(&rhei, &machine);
        assert!(report.has_errors(), "expected counted suffix validation error");
        assert!(
            report.errors.iter().any(|err| err.contains("Visit suffix '-1' is not allowed")),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn rejects_counted_state_suffix_when_state_has_no_visits() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending-2
"#;
        let rhei = parse(input).expect("parse ok");
        let machine = sample_machine();

        let report = validate_with_machine(&rhei, &machine);
        assert!(report.has_errors(), "expected counted suffix validation error");
        assert!(
            report.errors.iter().any(|err| err.contains("does not declare 'visits'")),
            "unexpected errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn reports_missing_numeric_dependency() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 9

### Task 2: B
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Example
## Tasks

### Task build: Build step
**State:** pending
**Prior:** Task deploy

### Task test: Test step
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Example
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
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn missing_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
"#;
        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("missing mandatory **State:**"),
            "expected parse error about missing state; got: {}",
            err.message
        );
    }

    #[test]
    fn reports_invalid_state_with_allowed_list() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** invalid_state
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
            assert!(joined.contains(s), "allowed list missing state '{}'; errors:\n{}", s, joined);
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
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** `in progress`
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &machine);

        assert!(
            !report.has_errors(),
            "unexpected errors validating escaped-space state: {:?}",
            report.errors
        );
    }

    #[test]
    fn ok_when_all_tasks_have_valid_state() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress

### Task 3: C
**State:** completed
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn detects_two_node_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Ex
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
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Ex
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
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors in DAG case: {:?}", report.errors);
    }

    #[test]
    fn no_false_cycle_with_missing_dependency() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 9

### Task 2: B
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Example
## Tasks

### Task 2: B
**State:** pending

#### Subtask 1.1: Wrong parent number
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

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
        let input = r#"# Rhei: Example
## Tasks

### Task 3: C
**State:** pending

#### Subtask 3.1: First
**State:** pending
#### Subtask 3.2: Second
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn terminal_parent_with_non_terminal_subtask_errors() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Subtask 2.1: Still open
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: terminal-parent-test
version: 1.0
states:
  pending: { description: "not started" }
  completed: { description: "done", final: true }
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected terminal parent coherence error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 2 is in terminal state 'completed'"),
            "expected terminal parent state in error; got:\n{}",
            joined
        );
        assert!(
            joined.contains("Subtask 2.1 ('Still open') is in non-terminal state 'pending'"),
            "expected non-terminal subtask in error; got:\n{}",
            joined
        );
    }

    #[test]
    fn terminal_parent_with_terminal_subtasks_is_valid() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Subtask 2.1: Done
**State:** completed
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: terminal-parent-test
version: 1.0
states:
  pending: { description: "not started" }
  completed: { description: "done", final: true }
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(
            !report.has_errors(),
            "terminal parent with terminal subtasks should validate: {:?}",
            report.errors
        );
    }

    #[test]
    fn named_task_subtasks_produce_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task build: B
**State:** pending

#### Subtask 1.1: Any number
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "named task with subtasks should produce errors");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 'build' has a named id and must not declare subtasks"),
            "expected named-task subtask error; errors were:\n{}",
            joined
        );
    }

    #[test]
    fn mixed_tasks_ok_and_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

#### Subtask 1.1: Correct
**State:** pending

### Task 2: B
**State:** pending

#### Subtask 1.2: Incorrect parent
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let mut report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected exactly one numbering error; got none");
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
        let input = r#"# Rhei: Example
## Tasks

### Task 4: D
**State:** pending

#### Subtask 4.1: Correct
**State:** pending
#### Subtask 3.2: Incorrect parent
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected at least one numbering error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Subtask 3.2") && joined.contains("under Task 4"),
            "expected mismatch message for 3.2 under Task 4; got:\n{}",
            joined
        );
    }

    #[test]
    fn prior_without_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**Prior:** Task 2

### Task 2: B
**State:** pending
"#;
        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("**State:** must appear before **Prior:**"),
            "expected parse error about ordering; got: {}",
            err.message
        );
    }

    #[test]
    fn validation_report_extend_merges_errors_and_warnings() {
        let mut base =
            ValidationReport { errors: vec!["e1".to_string()], warnings: vec!["w1".to_string()] };
        let other =
            ValidationReport { errors: vec!["e2".to_string()], warnings: vec!["w2".to_string()] };

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

    // ---- Markdown link validation tests ----

    #[test]
    fn extract_markdown_links_finds_all_links() {
        let text = "See [docs](docs/spec.md) and [site](https://example.com) for details.";
        let links = extract_markdown_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], ("docs".to_string(), "docs/spec.md".to_string()));
        assert_eq!(links[1], ("site".to_string(), "https://example.com".to_string()));
    }

    #[test]
    fn extract_markdown_links_handles_no_links() {
        let links = extract_markdown_links("No links here.");
        assert!(links.is_empty());
    }

    #[test]
    fn is_non_file_link_classifies_correctly() {
        assert!(is_non_file_link("https://example.com"));
        assert!(is_non_file_link("http://example.com"));
        assert!(is_non_file_link("mailto:user@example.com"));
        assert!(is_non_file_link("#section"));
        assert!(!is_non_file_link("docs/spec.md"));
        assert!(!is_non_file_link("../README.md"));
    }

    #[test]
    fn link_validation_reports_missing_file() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = r#"# Rhei: Example
## Overview
See [the spec](specs/nonexistent.md) for details.

## Tasks

### Task 1: A
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(report.has_errors(), "expected missing link error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("nonexistent.md") && joined.contains("does not exist"),
            "expected broken link error; got:\n{}",
            joined
        );
    }

    #[test]
    fn link_validation_passes_when_file_exists() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let specs_dir = dir.path().join("specs");
        fs::create_dir_all(&specs_dir).expect("mkdir");
        fs::write(specs_dir.join("real.md"), "# Real spec").expect("write");

        let input = r#"# Rhei: Example
## Overview
See [the spec](specs/real.md) for details.

## Tasks

### Task 1: A
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn link_validation_ignores_external_urls() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [docs](https://example.com/docs) and [anchor](#overview) for info.
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "external links should not be checked: {:?}", report.errors);
    }

    #[test]
    fn link_validation_strips_fragment_from_file_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(dir.path().join("guide.md"), "# Guide").expect("write");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [section](guide.md#usage) for details.
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(
            !report.has_errors(),
            "file exists, fragment should be stripped: {:?}",
            report.errors
        );
    }

    #[test]
    fn link_validation_checks_task_and_subtask_content() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [missing](nowhere.md) for context.

#### Subtask 1.1: Sub
**State:** pending
Also see [gone](also-gone.md).
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = Validator::new(sm).validate_with_base(&rhei, Some(dir.path()));

        assert!(report.has_errors());
        let joined = report.errors.join("\n");
        assert!(joined.contains("nowhere.md"), "should report task link; got:\n{}", joined);
        assert!(joined.contains("also-gone.md"), "should report subtask link; got:\n{}", joined);
    }

    #[test]
    fn link_validation_skipped_without_base_path() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

See [missing](nowhere.md) for context.
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        // validate() does not pass a base path, so link checking is skipped
        let report = validate_with_machine(&rhei, &sm);

        assert!(
            !report.has_errors(),
            "without base path, links should not be checked: {:?}",
            report.errors
        );
    }

    #[test]
    fn rejects_program_on_gating_state() {
        let yaml = r#"name: demo
version: 1
states:
  review:
    description: Human review
    gating: true
    program: "echo nope"
"#;

        let err = StateMachine::from_yaml_str(yaml).expect_err("should reject program on gating");
        assert!(err.to_string().contains("cannot declare a 'program'"));
    }

    #[test]
    fn rejects_exit_code_transition_from_non_program_state() {
        let yaml = r#"name: demo
version: 1
states:
  pending:
    description: Agent work
    agent: codex
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
    exit_code: 0
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject exit_code on non-program");
        assert!(err.to_string().contains("declares 'exit_code'"));
    }
}
