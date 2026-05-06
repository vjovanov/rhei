//! Semantic validation for parsed Rhei markdown plans.
//!
//! This crate provides two main pieces:
//! - [`StateMachine`], loaded from YAML, which defines allowed task states
//! - validation helpers such as [`validate_with_machine`] and
//!   [`validate_from_machine_file`] that check a parsed
//!   [`rhei_core::ast::Rhei`]
//!
//! The current validator enforces the behaviors implemented in this repository:
//! dependency existence, required `**State:**` metadata, state validity,
//! `**State:**` before `**Prior:**`, circular dependency detection,
//! ancestor-as-prior rejection, subtask parent-number consistency, and
//! terminal parent/subtask coherence.

use indexmap::IndexMap;
use regex::Regex;
pub use rhei_core::ast::{CallbackRef, StateName, TransitionRule};
use rhei_core::ast::{Rhei, Task, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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

/// Reference to an agent defined in the `agents` registry.
///
/// In `states.yaml` (`agent:`) and `settings.json` (`defaults.agent`), agents
/// are always referenced by string id. The concrete transport profile lives
/// in the `agents` registry in `settings.json` (global or project). See ADR
/// 0003 for the rationale.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AgentConfig(pub String);

impl AgentConfig {
    /// Return the agent identifier.
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl From<String> for AgentConfig {
    fn from(id: String) -> Self {
        AgentConfig(id)
    }
}

impl From<&str> for AgentConfig {
    fn from(id: &str) -> Self {
        AgentConfig(id.to_string())
    }
}

/// Agent transport profile. An entry in the `agents` registry in
/// `settings.json` (global or project), or the value of a built-in profile.
///
/// The registry key is the agent id; the id is not repeated inside this
/// value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct CustomAgentProfile {
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
    /// Flag used to attach one MCP server per occurrence (e.g., `"--mcp"`).
    /// Mutually exclusive with `mcp_config_flag`.
    #[serde(default)]
    pub mcp_flag: Option<String>,
    /// Flag used to attach a generated MCP config file (e.g., `"--mcp-config"`).
    /// Mutually exclusive with `mcp_flag`.
    #[serde(default)]
    pub mcp_config_flag: Option<String>,
    /// Flag used to enable one skill per occurrence (e.g., `"--skill"`).
    /// Omit to declare the agent does not support skills.
    #[serde(default)]
    pub skill_flag: Option<String>,
    /// Named modes. Each mode is an ordered flag list appended to the
    /// command at spawn time. A well-known mode name is `yolo`, but any
    /// name is allowed — Rhei does not interpret mode names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modes: BTreeMap<String, Vec<String>>,
}

/// Registry entry for an MCP server profile.
///
/// An entry must declare exactly one of `command` (local subprocess) or `url`
/// (remote transport). This is enforced at load time by the profile validator.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct McpServerProfile {
    /// Command and arguments to launch a local MCP server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// URL of a remote MCP server. Requires `transport`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Transport for remote servers (`"sse"`, `"websocket"`). Ignored for command-based servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Environment variables for the server process. Values may reference host env via `${VAR}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Working directory for the server process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Maximum time to wait for the server's MCP handshake (e.g., `"10s"`). Default: `"10s"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<String>,
}

/// Registry entry for a skill profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SkillProfile {
    /// Filesystem path to the skill bundle. Leading `~` expands to the user's home directory.
    pub path: String,
    /// Human-readable description of the skill's purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One entry in a state-level `mcp_servers` list or in `defaults.mcp_servers`.
///
/// Strings are registry ids with `optional: false`; objects allow `optional: true`
/// and inline definitions that do not require a registry entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StateMcpEntry {
    /// Shorthand for `{ id: "<name>" }` with `optional: false`.
    Id(String),
    /// Full object form with optional inline definition fields.
    Object(StateMcpEntryObject),
}

/// Object form of a state-level MCP entry.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct StateMcpEntryObject {
    /// Stable identifier. Must match a registry entry unless inline fields are provided.
    pub id: String,
    /// When `true`, a missing server does not block agent spawn.
    #[serde(default)]
    pub optional: bool,
    /// Inline command form (mutually exclusive with `url`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Inline url form (mutually exclusive with `command`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<String>,
}

impl StateMcpEntry {
    /// Registry id for this entry (always present).
    pub fn id(&self) -> &str {
        match self {
            StateMcpEntry::Id(id) => id,
            StateMcpEntry::Object(obj) => &obj.id,
        }
    }

    /// Whether this entry is marked optional.
    pub fn is_optional(&self) -> bool {
        match self {
            StateMcpEntry::Id(_) => false,
            StateMcpEntry::Object(obj) => obj.optional,
        }
    }

    /// Whether this entry carries an inline definition (rather than referring to a registry id).
    pub fn is_inline(&self) -> bool {
        match self {
            StateMcpEntry::Id(_) => false,
            StateMcpEntry::Object(obj) => obj.command.is_some() || obj.url.is_some(),
        }
    }
}

/// One entry in a state-level `skills` list or in `defaults.skills`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StateSkillEntry {
    /// Shorthand for `{ id: "<name>" }` with `optional: false`.
    Id(String),
    /// Full object form with optional inline definition fields.
    Object(StateSkillEntryObject),
}

/// Object form of a state-level skill entry.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct StateSkillEntryObject {
    pub id: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl StateSkillEntry {
    pub fn id(&self) -> &str {
        match self {
            StateSkillEntry::Id(id) => id,
            StateSkillEntry::Object(obj) => &obj.id,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            StateSkillEntry::Id(_) => false,
            StateSkillEntry::Object(obj) => obj.optional,
        }
    }

    pub fn is_inline(&self) -> bool {
        match self {
            StateSkillEntry::Id(_) => false,
            StateSkillEntry::Object(obj) => obj.path.is_some(),
        }
    }
}

/// Parsed inline execution target selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionTarget {
    /// Agent id that executes the target.
    pub agent: String,
    /// Optional named mode selected on the agent.
    pub mode: Option<String>,
    /// Optional provider segment carried by the selector.
    pub provider: Option<String>,
    /// Model identifier segment carried by the selector.
    pub model: String,
}

impl ExecutionTarget {
    /// Return a filesystem-safe slug for this selector.
    pub fn slug(&self) -> String {
        let mut slug = String::new();
        let mut last_was_dash = false;

        for ch in self.selector().chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                last_was_dash = false;
            } else if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }

        slug.trim_matches('-').to_string()
    }

    /// Reconstruct the normalized selector string.
    pub fn selector(&self) -> String {
        let mut selector = self.agent.clone();
        if let Some(mode) = &self.mode {
            selector.push('[');
            selector.push_str(mode);
            selector.push(']');
        }
        selector.push(':');
        if let Some(provider) = &self.provider {
            selector.push_str(provider);
            selector.push(':');
        }
        selector.push_str(&self.model);
        selector
    }
}

/// Parse an inline execution target selector.
pub fn parse_execution_target(selector: &str) -> Result<ExecutionTarget, String> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err("execution target selector must not be empty".to_string());
    }

    let parts: Vec<&str> = selector.split(':').collect();
    if parts.len() != 2 && parts.len() != 3 {
        return Err(format!(
            "execution target selector '{selector}' must use '<agent>:<model>', \
             '<agent>[<mode>]:<model>', '<agent>:<provider>:<model>', or \
             '<agent>[<mode>]:<provider>:<model>'"
        ));
    }

    let head = parts[0].trim();
    let (provider, model) = if parts.len() == 2 {
        (None, parts[1].trim())
    } else {
        (Some(parts[1].trim()), parts[2].trim())
    };

    if model.is_empty() {
        return Err(format!(
            "execution target selector '{selector}' must include a non-empty model"
        ));
    }
    if let Some(provider) = provider {
        if provider.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' must include a non-empty provider"
            ));
        }
    }

    let (agent, mode) = if let Some(open) = head.find('[') {
        if !head.ends_with(']') {
            return Err(format!(
                "execution target selector '{selector}' has an unterminated mode segment"
            ));
        }
        let agent = head[..open].trim();
        let mode = head[open + 1..head.len() - 1].trim();
        if agent.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' must include a non-empty agent"
            ));
        }
        if mode.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' must include a non-empty mode"
            ));
        }
        if mode.contains('[') || mode.contains(']') {
            return Err(format!(
                "execution target selector '{selector}' contains nested mode brackets"
            ));
        }
        (agent, Some(mode))
    } else {
        let agent = head.trim();
        if agent.is_empty() {
            return Err(format!(
                "execution target selector '{selector}' must include a non-empty agent"
            ));
        }
        if agent.contains(']') {
            return Err(format!(
                "execution target selector '{selector}' contains an unexpected ']'"
            ));
        }
        (agent, None)
    };

    Ok(ExecutionTarget {
        agent: agent.to_string(),
        mode: mode.map(str::to_string),
        provider: provider.map(str::to_string),
        model: model.to_string(),
    })
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
    /// When `true`, `rhei run` may work multiple ready tasks in this state
    /// simultaneously (bounded by `--parallel`). When `false`, at most one
    /// task per pass is scheduled for this state; remaining tasks are
    /// deferred to a later pass.
    #[serde(default)]
    pub concurrent: bool,
    /// Optional polling configuration. When present, the state is treated
    /// as a time-triggered state: a self-loop transition is interpreted as
    /// "retry after `poll.interval`", the `--parallel` slot is released
    /// between attempts, and the state's visit counter is capped at
    /// `poll.max_attempts`. Mutually exclusive with `visits`.
    #[serde(default)]
    pub poll: Option<PollConfig>,
    /// Optional visit budget for returning to this state.
    pub visits: Option<u32>,
    /// Inline execution target selector for one run of the state.
    #[serde(default)]
    pub target: Option<String>,
    /// Explicit list of execution target selectors for fanout execution.
    #[serde(default)]
    pub all_targets: Vec<String>,
    /// Explicit list of declared models that should each execute this state.
    #[serde(default)]
    pub all_models: Vec<String>,
    /// Restricts this state to one declared model.
    #[serde(default)]
    pub model: Option<String>,
    /// The coding agent CLI that executes work in this state. Must be a
    /// string id resolved against the merged `agents` registry (built-ins →
    /// global → project).
    #[serde(default)]
    pub agent: Option<AgentConfig>,
    /// Optional agent mode (named flag set) applied for this state. Must
    /// name a key in the resolved agent's `modes` map, if any.
    #[serde(default)]
    pub agent_mode: Option<String>,
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
    /// MCP servers attached to the agent subprocess in this state.
    ///
    /// `None` = field omitted → inherit `defaults.mcp_servers` unchanged.
    /// `Some(vec![])` = explicitly clear inherited defaults for this state.
    /// `Some(non-empty)` = state-level entries override/extend defaults by id.
    #[serde(default)]
    pub mcp_servers: Option<Vec<StateMcpEntry>>,
    /// Agent skills enabled for this state. Same tri-state semantics as `mcp_servers`.
    #[serde(default)]
    pub skills: Option<Vec<StateSkillEntry>>,
}

/// Per-state polling configuration. See the States Specification —
/// Polling States.
#[derive(Debug, Clone, Deserialize)]
pub struct PollConfig {
    /// Minimum wall-clock wait between poll attempts (duration string, e.g.
    /// `30s`, `5m`, `1h`).
    pub interval: String,
    /// Upper bound on total attempts for this state within one task
    /// lifetime. Must be `>= 1`.
    pub max_attempts: u32,
}

/// A named, reusable `{initial, allowed}` state policy referenced from
/// [`NodePolicy`]. See the States Specification — Profiles section.
#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    /// Initial state that nodes bound to this profile start in.
    pub initial: String,
    /// Complete set of state names that nodes bound to this profile may hold.
    pub allowed: Vec<String>,
}

/// Node-policy resolution: maps node kinds and id patterns to named profiles.
///
/// See the States Specification — Node Policy section for the resolution order
/// (`overrides` → `by_type[<kind>]` → `default`) and validation rules.
#[derive(Debug, Clone, Deserialize)]
pub struct NodePolicy {
    /// Profile bound to the plan-root node (always the `rhei` kind).
    pub root: String,
    /// Fallback profile for non-root nodes that match neither `overrides`
    /// nor `by_type`.
    pub default: String,
    /// Optional map from declared node kind to profile name.
    #[serde(default)]
    pub by_type: IndexMap<String, String>,
    /// Optional ordered list of `{match, profile}` overrides that win over
    /// `by_type` and `default`.
    #[serde(default)]
    pub overrides: Vec<NodePolicyOverride>,
}

/// Ordered override in [`NodePolicy`]. `match` is a task id or glob.
#[derive(Debug, Clone, Deserialize)]
pub struct NodePolicyOverride {
    /// Task id or glob pattern this override applies to.
    #[serde(rename = "match")]
    pub pattern: String,
    /// Profile name bound to matched nodes.
    pub profile: String,
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
    /// Named reusable state policies referenced from `node_policy`.
    ///
    /// The current schema revision makes this required, but the field is
    /// decoded optionally so legacy YAML without profiles still loads. When
    /// present, [`NodePolicy`] must be present as well.
    #[serde(default)]
    pub profiles: Option<IndexMap<String, Profile>>,
    /// Node-policy block that binds nodes to profiles. See [`NodePolicy`].
    #[serde(default)]
    pub node_policy: Option<NodePolicy>,
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
        sm.validate_tooling_configuration()?;
        sm.validate_template_conditions()?;
        sm.validate_profiles_and_node_policy()?;
        sm.validate_poll_configuration()?;
        sm.validate_terminal_state_present()?;
        Ok(sm)
    }

    /// Reject state machines that declare zero terminal states. Without one,
    /// `rhei complete`, terminal-state filters, and prerequisite resolution
    /// cannot work correctly, and a forgotten or mistyped `final: true` is
    /// otherwise silently accepted.
    fn validate_terminal_state_present(&self) -> Result<(), StateMachineLoadError> {
        if self.states.values().any(|state| state.terminal) {
            return Ok(());
        }
        Err(StateMachineLoadError::Invalid(format!(
            "state machine '{}' declares no terminal states. Mark at least one \
             state with `final: true` (note: the field is `final`, not `terminal`).",
            self.name
        )))
    }

    /// Resolve the profile that applies to a node with the given kind and id.
    ///
    /// Resolution order matches the States Specification — Node Policy:
    /// `overrides` (first matching `pattern`) → `by_type[<kind>]` →
    /// `default`. Returns `None` when the machine does not declare
    /// `profiles` / `node_policy`.
    ///
    /// The `pattern` in `overrides` is currently matched as an exact
    /// task-id string. Glob semantics will follow when task ids carry
    /// hierarchical segments in the AST.
    pub fn profile_for(&self, kind: Option<&str>, task_id: Option<&str>) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        let resolved_name = if let Some(id) = task_id {
            policy
                .overrides
                .iter()
                .find(|ov| ov.pattern == id)
                .map(|ov| ov.profile.as_str())
                .or_else(|| kind.and_then(|k| policy.by_type.get(k).map(|s| s.as_str())))
                .unwrap_or(policy.default.as_str())
        } else {
            kind.and_then(|k| policy.by_type.get(k).map(|s| s.as_str()))
                .unwrap_or(policy.default.as_str())
        };
        profiles.get(resolved_name)
    }

    /// Resolve the profile bound to the plan-root node.
    pub fn root_profile(&self) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        profiles.get(policy.root.as_str())
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
            if state.target.is_some() && !state.all_targets.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot set both 'target' and 'all_targets'"
                )));
            }
            if (state.target.is_some() || !state.all_targets.is_empty())
                && (state.model.is_some()
                    || !state.all_models.is_empty()
                    || state.agent.is_some()
                    || state.agent_mode.is_some())
            {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot combine 'target' or 'all_targets' with \
                     'model', 'all_models', 'agent', or 'agent_mode'"
                )));
            }
            if let Some(selector) = state.target.as_deref() {
                parse_execution_target(selector).map_err(|message| {
                    StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'target': {message}"
                    ))
                })?;
            }
            if !state.all_targets.is_empty() {
                let mut seen_targets = HashSet::new();
                for selector in &state.all_targets {
                    let parsed = parse_execution_target(selector).map_err(|message| {
                        StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' has invalid 'all_targets' entry: {message}"
                        ))
                    })?;
                    let normalized = parsed.selector();
                    if !seen_targets.insert(normalized.clone()) {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' contains duplicate 'all_targets' entry '{normalized}'"
                        )));
                    }
                }
            }
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
                if agent.id().trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'agent' value"
                    )));
                }
            }
            if let Some(mode) = &state.agent_mode {
                if state.agent.is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares 'agent_mode' without declaring an 'agent'"
                    )));
                }
                if mode.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'agent_mode' value"
                    )));
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

    /// Validate the per-state `poll:` block: well-formed `interval` and
    /// `max_attempts`, mutually exclusive with `visits`, forbidden on
    /// final/gating states, and at least one self-loop transition is
    /// declared so the retry branch is reachable.
    fn validate_poll_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            let Some(poll) = state.poll.as_ref() else { continue };
            if parse_duration_secs(&poll.interval).is_none() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has poll.interval '{}' that is not a valid duration (expected e.g. '30s', '5m', '1h')",
                    poll.interval
                )));
            }
            if poll.max_attempts < 1 {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' has poll.max_attempts {} (must be >= 1)",
                    poll.max_attempts
                )));
            }
            if state.terminal {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' is final and cannot declare 'poll' (terminal states have no work to execute)"
                )));
            }
            if state.gating {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' is gating and cannot declare 'poll' (gating states require human action; polling executes autonomously)"
                )));
            }
            if state.visits.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares both 'poll' and 'visits'; poll.max_attempts replaces the visits cap"
                )));
            }
            let has_self_loop =
                self.transitions.iter().any(|t| t.from.0 == *state_name && t.to.0 == *state_name);
            if !has_self_loop {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'poll' but has no self-loop transition; add a transition with from: {state_name} and to: {state_name} so the retry branch is reachable"
                )));
            }
        }
        Ok(())
    }

    /// Validate the per-state `mcp_servers` and `skills` lists and the
    /// matching `mcp_unavailable` / `skill_unavailable` transition triggers.
    ///
    /// This pass is purely structural — it rejects malformed entries,
    /// duplicate ids, and the gating/program/final exclusions. Cross-file
    /// reference resolution against settings registries happens elsewhere
    /// (the CLI merges settings and checks id resolution at load time).
    fn validate_tooling_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            validate_state_mcp_entries(state_name, state)?;
            validate_state_skill_entries(state_name, state)?;
        }

        for transition in &self.transitions {
            validate_transition_tooling_trigger(
                transition,
                transition.mcp_unavailable.as_ref(),
                "mcp_unavailable",
            )?;
            validate_transition_tooling_trigger(
                transition,
                transition.skill_unavailable.as_ref(),
                "skill_unavailable",
            )?;

            if transition.mcp_unavailable.is_some() || transition.skill_unavailable.is_some() {
                if let Some(from_state) = self.states.get(&transition.from.0) {
                    if from_state.program.is_some() {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "transition from '{}' to '{}' declares a tooling-unavailable trigger \
                             but source state '{}' is a program state (tooling triggers are agent-only)",
                            transition.from.0, transition.to.0, transition.from.0
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate the `profiles` and `node_policy` blocks introduced by the
    /// current schema revision.
    ///
    /// When both are absent, the machine is treated as legacy and no
    /// additional checks run. When either is present, both must be present
    /// and consistent:
    ///
    /// - `profiles` is non-empty.
    /// - Every profile's `initial` names a defined state and is a member of
    ///   its own `allowed` list; every `allowed` entry names a defined state;
    ///   `allowed` has no duplicates.
    /// - `node_policy.root` and `node_policy.default` name defined profiles.
    /// - Every `by_type` value names a defined profile. Keys must be
    ///   non-empty and unique; `rhei` is reserved and rejected here.
    /// - Every `overrides` entry has a non-empty `match` and names a defined
    ///   profile.
    fn validate_profiles_and_node_policy(&self) -> Result<(), StateMachineLoadError> {
        match (self.profiles.as_ref(), self.node_policy.as_ref()) {
            (None, None) => return Ok(()),
            (Some(_), None) => {
                return Err(StateMachineLoadError::Invalid(
                    "state machine declares 'profiles' but no 'node_policy' block".to_string(),
                ));
            }
            (None, Some(_)) => {
                return Err(StateMachineLoadError::Invalid(
                    "state machine declares 'node_policy' but no 'profiles' block".to_string(),
                ));
            }
            (Some(_), Some(_)) => {}
        }

        let profiles = self.profiles.as_ref().expect("profiles present");
        let policy = self.node_policy.as_ref().expect("node_policy present");

        if profiles.is_empty() {
            return Err(StateMachineLoadError::Invalid(
                "'profiles' must declare at least one profile".to_string(),
            ));
        }

        for (profile_name, profile) in profiles {
            if profile_name.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "'profiles' contains an entry with an empty name".to_string(),
                ));
            }

            if profile.initial.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' declares an empty 'initial'"
                )));
            }

            if !self.states.contains_key(&profile.initial) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' has 'initial: {}' but no such state is defined",
                    profile.initial
                )));
            }

            if profile.allowed.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' declares an empty 'allowed' list"
                )));
            }

            let mut seen = HashSet::new();
            for allowed in &profile.allowed {
                let trimmed = allowed.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{profile_name}' contains an empty entry in 'allowed'"
                    )));
                }
                if !seen.insert(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{profile_name}' contains duplicate 'allowed' entry '{trimmed}'"
                    )));
                }
                if !self.states.contains_key(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "profile '{profile_name}' lists unknown state '{trimmed}' in 'allowed'"
                    )));
                }
            }

            if !profile.allowed.iter().any(|s| s == &profile.initial) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "profile '{profile_name}' 'initial: {}' is not in its own 'allowed' list",
                    profile.initial
                )));
            }
        }

        if !profiles.contains_key(&policy.root) {
            return Err(StateMachineLoadError::Invalid(format!(
                "'node_policy.root' references undefined profile '{}'",
                policy.root
            )));
        }
        if !profiles.contains_key(&policy.default) {
            return Err(StateMachineLoadError::Invalid(format!(
                "'node_policy.default' references undefined profile '{}'",
                policy.default
            )));
        }

        let mut seen_kinds = HashSet::new();
        for (kind, profile_name) in &policy.by_type {
            let trimmed_kind = kind.trim();
            if trimmed_kind.is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "'node_policy.by_type' contains an empty node-kind key".to_string(),
                ));
            }
            if trimmed_kind.eq_ignore_ascii_case("rhei") {
                return Err(StateMachineLoadError::Invalid(
                    "'node_policy.by_type' must not declare the reserved kind 'rhei' \
                     (the root node is bound via 'node_policy.root')"
                        .to_string(),
                ));
            }
            if !seen_kinds.insert(trimmed_kind.to_ascii_lowercase()) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.by_type' contains duplicate kind '{trimmed_kind}' \
                     (kind matching is case-insensitive)"
                )));
            }
            if !profiles.contains_key(profile_name) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.by_type.{trimmed_kind}' references undefined profile \
                     '{profile_name}'"
                )));
            }
        }

        for (idx, ov) in policy.overrides.iter().enumerate() {
            if ov.pattern.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.overrides[{idx}]' has an empty 'match'"
                )));
            }
            if !profiles.contains_key(&ov.profile) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "'node_policy.overrides[{idx}]' references undefined profile '{}'",
                    ov.profile
                )));
            }
        }

        for (state_name, state) in &self.states {
            if state.initial {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'initial: true', but the machine uses \
                     'profiles' — the initial state is a property of each profile"
                )));
            }
        }

        Ok(())
    }

    /// Validate that every `{if <condition>}` tag in `instructions` and
    /// `personality` fields references a condition the engine can evaluate.
    ///
    /// Supported forms:
    /// - `input.<name>.exists` — `<name>` must be a declared input artifact
    ///   on the same state.
    /// - `mcp.<name>.available` — `<name>` must appear in the state's
    ///   `mcp_servers` list (including `defaults.mcp_servers` ids when
    ///   inherited is not cleared; this layer only checks the state-level
    ///   declaration since defaults live in settings, not the machine).
    /// - `skill.<id>.available` — same rule for the `skills` list.
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
                    } else if let Some(mcp_id) =
                        condition.strip_prefix("mcp.").and_then(|s| s.strip_suffix(".available"))
                    {
                        let declared = state
                            .mcp_servers
                            .as_deref()
                            .map(|entries| entries.iter().any(|e| e.id() == mcp_id))
                            .unwrap_or(false);
                        if !declared {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' {field_name} contains \
                                 '{{if {condition}}}' but '{mcp_id}' is not declared in this state's \
                                 'mcp_servers' list"
                            )));
                        }
                    } else if let Some(skill_id) =
                        condition.strip_prefix("skill.").and_then(|s| s.strip_suffix(".available"))
                    {
                        let declared = state
                            .skills
                            .as_deref()
                            .map(|entries| entries.iter().any(|e| e.id() == skill_id))
                            .unwrap_or(false);
                        if !declared {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' {field_name} contains \
                                 '{{if {condition}}}' but '{skill_id}' is not declared in this state's \
                                 'skills' list"
                            )));
                        }
                    } else {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' {field_name} contains \
                             '{{if {condition}}}' which is not a recognised condition; \
                             supported forms: 'input.<name>.exists', 'mcp.<name>.available', 'skill.<id>.available'"
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

fn validate_state_mcp_entries(
    state_name: &str,
    state: &StateDef,
) -> Result<(), StateMachineLoadError> {
    let Some(entries) = state.mcp_servers.as_deref() else {
        return Ok(());
    };

    if !entries.is_empty() {
        if state.gating {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is gating and cannot declare 'mcp_servers' (gating states are human-only)"
            )));
        }
        if state.program.is_some() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' declares 'program' and cannot declare 'mcp_servers' (programs execute deterministically)"
            )));
        }
        if state.terminal {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is final and cannot declare 'mcp_servers' (terminal states have no work)"
            )));
        }
    }

    let mut seen = HashSet::new();
    for entry in entries {
        let id = entry.id();
        if id.trim().is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has an mcp_servers entry with an empty id"
            )));
        }
        if !seen.insert(id.to_string()) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has a duplicate mcp_servers id '{id}'"
            )));
        }
        if let StateMcpEntry::Object(obj) = entry {
            if obj.command.is_some() && obj.url.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' mcp_servers entry '{id}' declares both 'command' and 'url' (mutually exclusive)"
                )));
            }
            if let Some(command) = &obj.command {
                if command.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' mcp_servers entry '{id}' has an empty 'command'"
                    )));
                }
            }
            if let Some(url) = &obj.url {
                if url.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' mcp_servers entry '{id}' has an empty 'url'"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_state_skill_entries(
    state_name: &str,
    state: &StateDef,
) -> Result<(), StateMachineLoadError> {
    let Some(entries) = state.skills.as_deref() else {
        return Ok(());
    };

    if !entries.is_empty() {
        if state.gating {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is gating and cannot declare 'skills' (gating states are human-only)"
            )));
        }
        if state.program.is_some() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' declares 'program' and cannot declare 'skills' (programs execute deterministically)"
            )));
        }
        if state.terminal {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is final and cannot declare 'skills' (terminal states have no work)"
            )));
        }
    }

    let mut seen = HashSet::new();
    for entry in entries {
        let id = entry.id();
        if id.trim().is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has a skills entry with an empty id"
            )));
        }
        if !seen.insert(id.to_string()) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has a duplicate skills id '{id}'"
            )));
        }
        if let StateSkillEntry::Object(obj) = entry {
            if let Some(path) = &obj.path {
                if path.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' skills entry '{id}' has an empty 'path'"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Validate the shape of a tooling-unavailable trigger: either `true` or a
/// non-empty list of non-empty string ids. `false` and other shapes are rejected.
fn validate_transition_tooling_trigger(
    transition: &TransitionRule,
    value: Option<&serde_yaml::Value>,
    field_name: &str,
) -> Result<(), StateMachineLoadError> {
    let Some(value) = value else { return Ok(()) };
    match value {
        serde_yaml::Value::Bool(true) => Ok(()),
        serde_yaml::Value::Bool(false) => Err(StateMachineLoadError::Invalid(format!(
            "transition from '{}' to '{}' declares '{field_name}: false' — omit the field instead",
            transition.from.0, transition.to.0
        ))),
        serde_yaml::Value::Sequence(items) => {
            if items.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "transition from '{}' to '{}' declares an empty '{field_name}' list",
                    transition.from.0, transition.to.0
                )));
            }
            let mut seen = HashSet::new();
            for item in items {
                let Some(id) = item.as_str() else {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition from '{}' to '{}' '{field_name}' entries must be strings",
                        transition.from.0, transition.to.0
                    )));
                };
                if id.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition from '{}' to '{}' '{field_name}' contains an empty id",
                        transition.from.0, transition.to.0
                    )));
                }
                if !seen.insert(id.to_string()) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition from '{}' to '{}' '{field_name}' contains duplicate id '{id}'",
                        transition.from.0, transition.to.0
                    )));
                }
            }
            Ok(())
        }
        _ => Err(StateMachineLoadError::Invalid(format!(
            "transition from '{}' to '{}' '{field_name}' must be `true` or a list of ids",
            transition.from.0, transition.to.0
        ))),
    }
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
        validate_sibling_uniqueness(rhei, &mut report);
        validate_dependency_integrity(rhei, &index, &mut report);
        validate_state_consistency(rhei, &self.machine, &mut report);
        validate_terminal_tree_coherence(rhei, &self.machine, &mut report);
        validate_circular_dependencies(rhei, &index, &mut report);
        validate_assignee_nonempty(rhei, &mut report);

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
    fn recurse(
        task: &Task,
        ancestors: &mut Vec<TaskId>,
        index: &HashMap<TaskId, &Task>,
        report: &mut ValidationReport,
    ) {
        for dep in &task.prior {
            if !index.contains_key(dep) {
                report.errors.push(format!("Task {} depends on missing Task {}", task.id, dep));
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
            recurse(child, ancestors, index, report);
        }
        ancestors.pop();
    }

    let mut ancestors = Vec::new();
    for task in &rhei.tasks {
        recurse(task, &mut ancestors, index, report);
    }
}

fn validate_state_consistency(rhei: &Rhei, machine: &StateMachine, report: &mut ValidationReport) {
    for_each_node(rhei, |task| {
        let task_id_str = task.id.to_string();
        let kind_label = title_case_kind(&task.kind);
        let subject = format!("{} {}", kind_label, task.id);
        validate_task_state_instance(&subject, &task.state, machine, report);
        validate_task_state_against_profile(
            &subject,
            &task.state,
            Some(task.kind.as_str()),
            Some(task_id_str.as_str()),
            machine,
            report,
        );
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
    kind: Option<&str>,
    task_id: Option<&str>,
    machine: &StateMachine,
    report: &mut ValidationReport,
) {
    let Some(profile) = machine.profile_for(kind, task_id) else {
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

    for_each_node(rhei, |task| {
        for (display, target) in extract_markdown_links(&task.content) {
            let label = format!("{} {}", title_case_kind(&task.kind), task.id);
            links.push((label, display, target));
        }
    });

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
  done:
    description: done
    final: true
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
    fn parses_execution_target_with_mode_and_provider() {
        let target = parse_execution_target("claude-code[yolo]:anthropic:claude-opus-4-7")
            .expect("target should parse");

        assert_eq!(target.agent, "claude-code");
        assert_eq!(target.mode.as_deref(), Some("yolo"));
        assert_eq!(target.provider.as_deref(), Some("anthropic"));
        assert_eq!(target.model, "claude-opus-4-7");
        assert_eq!(target.slug(), "claude-code-yolo-anthropic-claude-opus-4-7");
    }

    #[test]
    fn loads_state_machine_with_target_selectors() {
        let yaml = r#"
name: multi-target
version: 1.0
states:
  analyze:
    description: analyze
    all_targets:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - codex[yolo]:openai:gpt-5-codex
  done:
    description: done
    final: true
"#;

        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        assert_eq!(machine.states["analyze"].all_targets.len(), 2);
        assert_eq!(
            machine.states["analyze"].all_targets[0],
            "claude-code[yolo]:anthropic:claude-opus-4-7"
        );
    }

    #[test]
    fn rejects_state_machine_with_conflicting_target_and_model_selectors() {
        let yaml = r#"
name: multi-target
version: 1.0
models:
  - gpt-5
states:
  analyze:
    description: analyze
    target: codex[yolo]:openai:gpt-5-codex
    model: gpt-5
"#;

        let err =
            StateMachine::from_yaml_str(yaml).expect_err("should reject conflicting selectors");
        assert!(err.to_string().contains("cannot combine 'target' or 'all_targets'"));
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
  done:
    description: done
    final: true
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
  done:
    description: done
    final: true
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
  done:
    description: done
    final: true
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

#### Task 1.1: s
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

#### Task 1.1: s
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
  done:
    description: done
    final: true
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
  done:
    description: done
    final: true
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
    fn rejects_child_prior_to_parent() {
        let input = r#"# Rhei: Example
## Tasks

### Task fetch-prs: Fetch pull requests
**State:** completed

#### Task fetch-prs.ci-failure-5227: Triage CI failure
**State:** pending
**Prior:** Task fetch-prs
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected parent-as-prior validation error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains(
                "Task fetch-prs.ci-failure-5227 cannot list ancestor Task fetch-prs as **Prior:**"
            ),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_descendant_prior_to_ancestor() {
        let input = r#"# Rhei: Example
---
structure:
  maxLevels: 3
---

## Tasks

### Task release: Release
**State:** pending

#### Task release.notes: Notes
**State:** pending

##### Task release.notes.diff: Diff notes
**State:** pending
**Prior:** Task release
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected ancestor-as-prior validation error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains(
                "Task release.notes.diff cannot list ancestor Task release as **Prior:**"
            ),
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
  done: { description: "done", final: true }
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

    // ---- Child/parent id-extension semantics ----
    //
    // The "subtask numbering" validator has been removed; the rule that a
    // child id must extend its parent's id by exactly one segment is now
    // enforced by the parser (see `crates/rhei-core/src/parser.rs`), which
    // rejects malformed child headings with a parse error before validation
    // runs. The old `mismatched_parent_number_errors`,
    // `named_task_subtasks_produce_error`, `mixed_tasks_ok_and_error`, and
    // `multiple_subtasks_some_bad` tests were deleted accordingly — their
    // inputs no longer parse, so there's nothing left for the validator to
    // check.

    #[test]
    fn valid_subtask_numbering_ok() {
        let input = r#"# Rhei: Example
## Tasks

### Task 3: C
**State:** pending

#### Task 3.1: First
**State:** pending
#### Task 3.2: Second
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

#### Task 2.1: Still open
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
            joined
                .contains("descendant Task 2.1 ('Still open') is in non-terminal state 'pending'"),
            "expected non-terminal descendant in error; got:\n{}",
            joined
        );
    }

    #[test]
    fn terminal_parent_with_terminal_subtasks_is_valid() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Task 2.1: Done
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
    fn duplicate_sibling_child_id_is_rejected() {
        // The new validator checks that sibling ids under a common parent are
        // unique, replacing the old ad-hoc "subtask uniqueness" rule.
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

#### Task 1.1: First
**State:** pending

#### Task 1.1: Duplicate
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "duplicate sibling id should be rejected");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Duplicate sibling task id: Task 1.1")
                && joined.contains("under Task 1"),
            "expected duplicate-sibling message; got:\n{}",
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

#### Task 1.1: Sub
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

    // ---- MCP servers / skills per-state validation ----

    #[test]
    fn state_mcp_servers_accepts_string_and_object_forms() {
        let yaml = r#"
name: mcp-basic
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - postgres
      - id: grafana
        optional: true
    skills:
      - test-authoring
      - id: adhoc
        path: ./skills/adhoc
        optional: true
  completed:
    description: Done
    final: true
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("should accept both forms");
        let pending = sm.states.get("pending").expect("pending state");
        let mcp = pending.mcp_servers.as_ref().expect("mcp_servers declared");
        assert_eq!(mcp.len(), 2);
        assert_eq!(mcp[0].id(), "postgres");
        assert!(!mcp[0].is_optional());
        assert_eq!(mcp[1].id(), "grafana");
        assert!(mcp[1].is_optional());

        let skills = pending.skills.as_ref().expect("skills declared");
        assert_eq!(skills.len(), 2);
        assert!(
            matches!(&skills[1], StateSkillEntry::Object(obj) if obj.path.as_deref() == Some("./skills/adhoc"))
        );
    }

    #[test]
    fn state_mcp_servers_empty_list_preserved_as_clear_marker() {
        let yaml = r#"
name: mcp-clear
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers: []
  completed:
    description: Done
    final: true
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("empty list is valid");
        let pending = sm.states.get("pending").expect("pending");
        assert_eq!(pending.mcp_servers.as_deref().map(<[_]>::len), Some(0));
    }

    #[test]
    fn state_mcp_servers_rejects_duplicate_ids() {
        let yaml = r#"
name: mcp-dup
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - postgres
      - id: postgres
        optional: true
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("duplicate ids");
        assert!(err.to_string().contains("duplicate mcp_servers id 'postgres'"));
    }

    #[test]
    fn state_mcp_servers_rejects_both_command_and_url() {
        let yaml = r#"
name: mcp-inline-both
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: inline
        command: ["mcp-server"]
        url: "https://example/mcp"
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("mutually exclusive");
        assert!(err.to_string().contains("both 'command' and 'url'"));
    }

    #[test]
    fn state_mcp_servers_rejected_on_gating_state() {
        let yaml = r#"
name: mcp-gating
version: 1.0
states:
  pending:
    description: Work
    gating: true
    mcp_servers: [postgres]
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("gating excludes mcp");
        assert!(err.to_string().contains("gating"));
    }

    #[test]
    fn state_mcp_servers_rejected_on_program_state() {
        let yaml = r#"
name: mcp-program
version: 1.0
states:
  build:
    description: Build
    program: "make"
    mcp_servers: [postgres]
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("program excludes mcp");
        assert!(err.to_string().contains("program"));
    }

    #[test]
    fn state_skills_rejected_on_terminal_state() {
        let yaml = r#"
name: skill-final
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
  completed:
    description: Done
    final: true
    skills: [review-checklist]
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("final excludes skills");
        assert!(err.to_string().contains("final"));
    }

    #[test]
    fn template_condition_accepts_mcp_and_skill_when_declared() {
        let yaml = r#"
name: cond-ok
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    instructions: |
      {if mcp.postgres.available}Use Postgres.{endif}
      {if skill.test-authoring.available}Use test skill.{endif}
    mcp_servers: [postgres]
    skills: [test-authoring]
  completed:
    description: Done
    final: true
"#;
        StateMachine::from_yaml_str(yaml).expect("valid references");
    }

    #[test]
    fn template_condition_rejects_mcp_not_declared() {
        let yaml = r#"
name: cond-bad-mcp
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    instructions: "{if mcp.other.available}X{endif}"
    mcp_servers: [postgres]
  completed:
    description: Done
    final: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("other is not declared");
        assert!(err.to_string().contains("'other'"));
        assert!(err.to_string().contains("mcp_servers"));
    }

    #[test]
    fn transition_mcp_unavailable_accepts_true_and_list() {
        let yaml = r#"
name: trig-ok
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
    mcp_servers: [postgres]
  tooling-missing:
    description: Blocked
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: tooling-missing
    mcp_unavailable: true
  - from: pending
    to: tooling-missing
    mcp_unavailable: [postgres]
"#;
        StateMachine::from_yaml_str(yaml).expect("valid trigger shapes");
    }

    #[test]
    fn transition_mcp_unavailable_rejects_false() {
        let yaml = r#"
name: trig-false
version: 1.0
states:
  pending:
    description: Work
    agent: claude-code
  tooling-missing:
    description: Blocked
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: tooling-missing
    mcp_unavailable: false
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("false is invalid");
        assert!(err.to_string().contains("mcp_unavailable: false"));
    }

    #[test]
    fn transition_mcp_unavailable_rejects_on_program_state() {
        let yaml = r#"
name: trig-prog
version: 1.0
states:
  build:
    description: Build
    program: "make"
  failed:
    description: Build failed
    final: true
transitions:
  - from: build
    to: failed
    mcp_unavailable: true
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("program source state");
        assert!(err.to_string().contains("agent-only"));
    }

    // ---- profiles / node_policy ----

    fn profiles_machine_yaml() -> &'static str {
        r#"
name: profiled
version: 3.0
states:
  pending:
    description: Work
  review:
    description: Inspect
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
profiles:
  default:
    initial: pending
    allowed: [pending, review, completed]
  fast-track:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
  by_type:
    bug: fast-track
"#
    }

    #[test]
    fn loads_profiles_and_node_policy() {
        let sm = StateMachine::from_yaml_str(profiles_machine_yaml()).expect("load ok");
        let profiles = sm.profiles.as_ref().expect("profiles present");
        assert_eq!(profiles.len(), 2);
        let default = sm.profile_for(Some("task"), Some("1")).expect("default resolves");
        assert_eq!(default.initial, "pending");
        let fast = sm.profile_for(Some("bug"), Some("2")).expect("bug resolves to fast-track");
        assert_eq!(fast.allowed, vec!["pending".to_string(), "completed".to_string()]);
        let root = sm.root_profile().expect("root profile");
        assert_eq!(root.initial, "pending");
    }

    #[test]
    fn profile_for_returns_none_when_not_declared() {
        let sm = sample_machine();
        assert!(sm.profile_for(Some("task"), None).is_none());
        assert!(sm.root_profile().is_none());
    }

    #[test]
    fn rejects_profiles_without_node_policy() {
        let yaml = r#"
name: half-config
version: 1.0
states:
  pending: { description: Work }
profiles:
  default:
    initial: pending
    allowed: [pending]
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("missing node_policy");
        assert!(err.to_string().contains("no 'node_policy'"));
    }

    #[test]
    fn rejects_node_policy_without_profiles() {
        let yaml = r#"
name: half-config
version: 1.0
states:
  pending: { description: Work }
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("missing profiles");
        assert!(err.to_string().contains("no 'profiles'"));
    }

    #[test]
    fn rejects_profile_with_initial_not_in_allowed() {
        let yaml = r#"
name: bad-initial
version: 1.0
states:
  pending: { description: Work }
  review: { description: Inspect }
profiles:
  default:
    initial: review
    allowed: [pending]
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("initial not in allowed");
        assert!(err.to_string().contains("is not in its own 'allowed' list"));
    }

    #[test]
    fn rejects_profile_with_unknown_state_in_allowed() {
        let yaml = r#"
name: unknown-allowed
version: 1.0
states:
  pending: { description: Work }
profiles:
  default:
    initial: pending
    allowed: [pending, missing]
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("unknown allowed");
        assert!(err.to_string().contains("unknown state 'missing'"));
    }

    #[test]
    fn rejects_node_policy_default_with_undefined_profile() {
        let yaml = r#"
name: dangling-default
version: 1.0
states:
  pending: { description: Work }
profiles:
  default:
    initial: pending
    allowed: [pending]
node_policy:
  root: default
  default: nonexistent
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("dangling profile");
        assert!(err.to_string().contains("'node_policy.default' references undefined profile"));
    }

    #[test]
    fn rejects_node_policy_by_type_with_reserved_kind() {
        let yaml = r#"
name: reserved-kind
version: 1.0
states:
  pending: { description: Work }
profiles:
  default:
    initial: pending
    allowed: [pending]
node_policy:
  root: default
  default: default
  by_type:
    rhei: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("reserved kind");
        assert!(err.to_string().contains("reserved kind 'rhei'"));
    }

    #[test]
    fn rejects_state_initial_true_when_profiles_present() {
        let yaml = r#"
name: legacy-initial
version: 1.0
states:
  pending:
    description: Work
    initial: true
profiles:
  default:
    initial: pending
    allowed: [pending]
node_policy:
  root: default
  default: default
"#;
        let err = StateMachine::from_yaml_str(yaml).expect_err("initial forbidden with profiles");
        assert!(err.to_string().contains("declares 'initial: true'"));
    }

    #[test]
    fn enforces_profile_allowed_on_task_state() {
        let sm = StateMachine::from_yaml_str(profiles_machine_yaml()).expect("load ok");
        let input = "# Rhei: profile-check\n**States:** profiled\n\n## Tasks\n\n### Task 1: First\n**State:** review\n";
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sm);
        assert!(!report.has_errors(), "review is allowed: {:?}", report.errors);
    }

    #[test]
    fn rejects_task_state_outside_profile_allowed() {
        // Build a machine where `default` profile excludes `review`, then
        // author a task in `review` — it's a defined state but disallowed
        // for this node.
        let yaml = r#"
name: restricted
version: 1.0
states:
  pending: { description: Work }
  review: { description: Inspect }
  completed: { description: Done, final: true }
profiles:
  default:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default
  default: default
"#;
        let sm = StateMachine::from_yaml_str(yaml).expect("load ok");
        let input = "# Rhei: restricted-check\n**States:** restricted\n\n## Tasks\n\n### Task 1: First\n**State:** review\n";
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &sm);
        assert!(
            report.errors.iter().any(|e| e.contains("not allowed by its resolved profile")),
            "expected profile-allowed error, got {:?}",
            report.errors
        );
    }

    fn poll_machine(body: &str) -> String {
        format!(
            r#"
name: poll-test
version: 1.0
states:
{body}
profiles:
  default:
    initial: ci-wait
    allowed: [ci-wait, done]
node_policy:
  root: default
  default: default
"#
        )
    }

    #[test]
    fn accepts_well_formed_poll_state() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait for CI
    program: "./check.sh"
    poll:
      interval: 5m
      max_attempts: 12
  done:
    description: Done
    final: true
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75
  - from: ci-wait
    to: done
    exit_code: 0"#,
        );
        let sm = StateMachine::from_yaml_str(&yaml).expect("valid poll state");
        let poll = sm.states.get("ci-wait").and_then(|s| s.poll.as_ref()).expect("poll present");
        assert_eq!(poll.max_attempts, 12);
        assert_eq!(poll.interval, "5m");
    }

    #[test]
    fn rejects_poll_with_invalid_interval() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: sometimes
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("bad interval");
        assert!(err.to_string().contains("poll.interval"));
    }

    #[test]
    fn rejects_poll_with_zero_max_attempts() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: 1m
      max_attempts: 0
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("bad max_attempts");
        assert!(err.to_string().contains("poll.max_attempts"));
    }

    #[test]
    fn rejects_poll_with_visits() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    visits: 5
    poll:
      interval: 1m
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait
    exit_code: 75"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("visits conflict");
        assert!(err.to_string().contains("'poll' and 'visits'"));
    }

    #[test]
    fn rejects_poll_on_gating_state() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    gating: true
    poll:
      interval: 1m
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: ci-wait"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("gating conflict");
        assert!(err.to_string().contains("gating"));
    }

    #[test]
    fn rejects_poll_without_self_loop() {
        let yaml = poll_machine(
            r#"  ci-wait:
    description: Wait
    program: "./check.sh"
    poll:
      interval: 1m
      max_attempts: 3
  done: { description: Done, final: true }
transitions:
  - from: ci-wait
    to: done
    exit_code: 0"#,
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("missing self-loop");
        assert!(err.to_string().contains("self-loop"));
    }
}
