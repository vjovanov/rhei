/// One entry from the `states` map in a YAML states file.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    // §FS-rhei-states.2: Poll state semantics.
    #[serde(default)]
    pub poll: Option<PollConfig>,
    /// Optional visit budget for returning to this state.
    pub visits: Option<u32>,
    /// Optional named snapshot emit/inherit declaration.
    ///
    /// The operational CLI and run override surface inspect this field to
    /// enforce that `--from-snapshot` only applies to states with an authored
    /// inherit contract. Full static snapshot validation is owned by
    /// the snapshot validation rules.
    // §FS-rhei-snapshots.11: Snapshot validation rules.
    #[serde(default)]
    pub snapshot: Option<StateSnapshotConfig>,
    /// Same-task state handoff prompt inheritance. Handoff artifacts are
    /// declared as `outputs` with `kind: handoff`; this field controls which
    /// previous-state handoffs are injected into the successor prompt.
    // §FS-rhei-states.3.2: State handoff inheritance grammar.
    #[serde(default)]
    pub handoff: Option<StateHandoffConfig>,
    /// Inline execution target selector for one run of the state.
    #[serde(default)]
    pub target: Option<String>,
    /// Explicit list of execution target selectors for fanout execution.
    #[serde(default)]
    pub all_targets: Vec<String>,
    /// When true, per-task execution overrides cannot replace this state's identity.
    #[serde(default)]
    pub target_locked: bool,
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

/// §FS-rhei-states.2.1: Per-state polling configuration shape.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PollConfig {
    /// Minimum wall-clock wait between poll attempts (duration string, e.g.
    /// `30s`, `5m`, `1h`).
    pub interval: String,
    /// Upper bound on total attempts for this state within one task
    /// lifetime. Must be `>= 1`.
    pub max_attempts: u32,
}

/// Per-state snapshot declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateSnapshotConfig {
    #[serde(default)]
    pub emit: Option<SnapshotEmitConfig>,
    #[serde(default)]
    pub inherit: Option<SnapshotInheritConfig>,
}

/// `snapshot.emit` declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEmitConfig {
    pub name: String,
    #[serde(default)]
    pub on: Option<String>,
}

/// `snapshot.inherit` declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotInheritConfig {
    pub name: String,
    #[serde(default, rename = "from")]
    pub from_axis: Option<String>,
    #[serde(default)]
    pub compat: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub select: Option<SnapshotInheritSelectConfig>,
}

/// `snapshot.inherit.select` declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotInheritSelectConfig {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub visit: Option<serde_yaml::Value>,
    #[serde(default)]
    pub generation: Option<serde_yaml::Value>,
}

/// Per-state handoff inheritance declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateHandoffConfig {
    #[serde(default)]
    pub inherit: Vec<HandoffInheritConfig>,
}

/// One inherited handoff source.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffInheritConfig {
    #[serde(rename = "from")]
    pub from_axis: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub merge: Option<String>,
}

fn validate_snapshot_name(
    state_name: &str,
    field: &str,
    value: &str,
) -> Result<(), StateMachineLoadError> {
    let valid = value.len() <= 64
        && value.bytes().next().is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(StateMachineLoadError::Invalid(format!(
            "state '{state_name}' has invalid {field} '{value}' (expected ^[a-z][a-z0-9-]*$, max 64 characters)"
        )))
    }
}

fn validate_snapshot_selector_value(
    state_name: &str,
    field: &str,
    value: &serde_yaml::Value,
    allowed_strings: &[&str],
) -> Result<(), StateMachineLoadError> {
    let valid = match value {
        serde_yaml::Value::String(value) => {
            allowed_strings.contains(&value.as_str())
                || value.parse::<u64>().is_ok_and(|number| number >= 1)
        }
        serde_yaml::Value::Number(number) => number.as_u64().is_some_and(|number| number >= 1),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(StateMachineLoadError::Invalid(format!(
            "state '{state_name}' has invalid {field} value '{value:?}'"
        )))
    }
}

fn state_declares_snapshot_target_shape(state: &StateDef) -> bool {
    state.target.is_some()
        || !state.all_targets.is_empty()
        || !state.all_models.is_empty()
        || state.model.is_some()
        || state.agent.is_some()
}

fn state_declares_snapshot_fanout_source(state: &StateDef) -> bool {
    !state.all_targets.is_empty() || !state.all_models.is_empty()
}

fn statically_resolved_snapshot_agent(state: &StateDef) -> Option<String> {
    if let Some(selector) = state.target.as_deref() {
        return parse_execution_target(selector).ok().map(|target| target.agent);
    }
    if !state.all_targets.is_empty() {
        let mut agents = state.all_targets.iter().filter_map(|selector| {
            parse_execution_target(selector).ok().map(|target| target.agent)
        });
        let first = agents.next()?;
        if agents.all(|agent| agent == first) {
            return Some(first);
        }
        return None;
    }
    state.agent.as_ref().map(|agent| agent.id().to_string())
}

/// §FS-rhei-states.8: Named reusable `{initial, allowed}` state policy.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Profile {
    /// Initial state that nodes bound to this profile start in.
    pub initial: String,
    /// Complete set of state names that nodes bound to this profile may hold.
    pub allowed: Vec<String>,
}

/// Node-policy resolution: maps node type/level selectors to named profiles.
///
/// Resolution order: `overrides`, `by_type[<kind>]`, then `default`.
// §FS-rhei-states.9.2: Node-policy resolution order.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// §FS-rhei-states.9.1: Ordered node-policy override selector.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodePolicyOverride {
    /// Type/level selector. An empty selector matches every non-root node.
    #[serde(rename = "match")]
    pub match_: NodePolicyMatch,
    /// Profile name bound to matched nodes.
    pub profile: String,
}

/// §FS-rhei-states.9.3: Reject unknown node-policy match keys.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodePolicyMatch {
    /// Optional node kind selector.
    #[serde(default, rename = "type")]
    pub node_type: Option<String>,
    /// Optional node level selector (`1` for a top-level task node).
    #[serde(default)]
    pub level: Option<u8>,
}

impl NodePolicyMatch {
    fn matches(&self, kind: &str, level: u8) -> bool {
        self.node_type.as_deref().is_none_or(|want| want.eq_ignore_ascii_case(kind))
            && self.level.is_none_or(|want| want == level)
    }
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
    // §FS-rhei-states.8: Profile map.
    #[serde(default)]
    pub profiles: Option<IndexMap<String, Profile>>,
    /// §FS-rhei-states.9: Node-policy block that binds nodes to profiles.
    #[serde(default)]
    pub node_policy: Option<NodePolicy>,
}

/// The built-in default states YAML shipped with rhei.
const DEFAULT_STATES_YAML: &str = include_str!("../default-states.yaml");
