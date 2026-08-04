impl StateMachine {
    /// Return the built-in default state machine shipped with rhei.
    pub fn builtin_default() -> Self {
        Self::from_yaml_str(DEFAULT_STATES_YAML).expect("built-in states YAML is always valid")
    }

    /// Load a StateMachine from YAML string contents. A machine declaring
    /// `extends:` is a composition layer: completeness validation is deferred
    /// to the folded result per §FS-rhei-states.1.3.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, StateMachineLoadError> {
        reject_explicit_empty_all_targets(yaml)?;
        let sm: Self = serde_yaml::from_str(yaml)?;
        if let Some(base) = sm.extends.as_deref() {
            if base.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state machine '{}' declares an empty `extends` value",
                    sm.name
                )));
            }
            return Ok(sm);
        }
        // §FS-rhei-states.1.3: `remove` requires `extends`.
        if !sm.remove.is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state machine '{}' declares `remove` without `extends`; \
                 removal only applies to inherited transitions",
                sm.name
            )));
        }
        sm.validate_complete()?;
        Ok(sm)
    }

    /// Run the full schema validation suite. Called on self-contained machines
    /// at load time and on the merged machine after `extends` folding.
    /// §FS-rhei-states.12.4
    pub fn validate_complete(&self) -> Result<(), StateMachineLoadError> {
        self.validate_model_configuration()?;
        self.validate_program_configuration()?;
        self.validate_snapshot_configuration()?;
        self.validate_tooling_configuration()?;
        self.validate_template_conditions()?;
        self.validate_poll_configuration()?;
        self.validate_profiles_and_node_policy()?;
        self.validate_terminal_state_present()?;
        Ok(())
    }

    /// Fold the `extends` chain into the effective, validated machine per
    /// §FS-rhei-states.12.1: acyclic, rooted at the built-in `rhei` (currently
    /// the only resolvable base). No-op without `extends`.
    pub fn into_effective(self) -> Result<Self, StateMachineLoadError> {
        let Some(base_name) = self.extends.clone() else {
            return Ok(self);
        };
        let base_name = base_name.trim().to_string();
        if base_name == self.name {
            return Err(StateMachineLoadError::Invalid(format!(
                "state machine '{}' extends itself; the `extends` chain must be acyclic",
                self.name
            )));
        }
        let builtin = Self::builtin_default();
        if base_name != builtin.name {
            return Err(StateMachineLoadError::Invalid(format!(
                "state machine '{}' extends '{}', which cannot be resolved: only the \
                 built-in '{}' machine is currently resolvable as an `extends` base",
                self.name, base_name, builtin.name
            )));
        }
        let effective = Self::compose(&builtin, &self)?;
        effective.validate_complete()?;
        Ok(effective)
    }

    /// Fold one layer onto a base per §FS-rhei-states.12.2: union per
    /// collection, whole-entity override, transitions merged by pair-group in
    /// base order. The result keeps the overlay's identity, sans extends.
    pub fn compose(base: &StateMachine, overlay: &StateMachine) -> Result<StateMachine, StateMachineLoadError> {
        let base_pairs: std::collections::HashSet<(&str, &str)> =
            base.transitions.iter().map(|t| (t.from.0.as_str(), t.to.0.as_str())).collect();

        // §FS-rhei-states.1.3: every `remove` entry must name an inherited
        // pair-group, and a layer must not both remove and restate a pair.
        for removal in &overlay.remove {
            if !base_pairs.contains(&(removal.from.as_str(), removal.to.as_str())) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state machine '{}' removes transition ({} -> {}) that no lower layer declares",
                    overlay.name, removal.from, removal.to
                )));
            }
            if overlay
                .transitions
                .iter()
                .any(|t| t.from.0 == removal.from && t.to.0 == removal.to)
            {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state machine '{}' both removes and declares transition ({} -> {}); \
                     restating replaces the inherited group, removing deletes it — declare one or the other",
                    overlay.name, removal.from, removal.to
                )));
            }
        }

        // States: union by name; a same-named state is replaced wholesale and
        // keeps its base position (IndexMap::insert preserves existing slots).
        let mut states = base.states.clone();
        for (name, def) in &overlay.states {
            states.insert(name.clone(), def.clone());
        }

        // §FS-rhei-states.12.2 pair-group union: a replacing group takes the
        // first replaced slot, new pairs append after inherited transitions,
        // removed groups vacate their position.
        let removed: std::collections::HashSet<(&str, &str)> =
            overlay.remove.iter().map(|r| (r.from.as_str(), r.to.as_str())).collect();
        let overlay_pairs: std::collections::HashSet<(&str, &str)> =
            overlay.transitions.iter().map(|t| (t.from.0.as_str(), t.to.0.as_str())).collect();
        let mut transitions: Vec<TransitionRule> = Vec::new();
        let mut replacement_emitted: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for rule in &base.transitions {
            let pair = (rule.from.0.as_str(), rule.to.0.as_str());
            if removed.contains(&pair) {
                continue;
            }
            if overlay_pairs.contains(&pair) {
                if replacement_emitted.insert((pair.0.to_string(), pair.1.to_string())) {
                    transitions.extend(
                        overlay
                            .transitions
                            .iter()
                            .filter(|o| o.from.0 == pair.0 && o.to.0 == pair.1)
                            .cloned(),
                    );
                }
                continue;
            }
            transitions.push(rule.clone());
        }
        for rule in &overlay.transitions {
            let pair = (rule.from.0.as_str(), rule.to.0.as_str());
            if !base_pairs.contains(&pair) {
                transitions.push(rule.clone());
            }
        }

        // Profiles: union by name; a same-named profile replaces the lower one
        // wholesale — `allowed` is never element-merged. §FS-rhei-states.12.2
        let profiles = match (&base.profiles, &overlay.profiles) {
            (None, None) => None,
            (base_profiles, overlay_profiles) => {
                let mut merged = base_profiles.clone().unwrap_or_default();
                if let Some(overlay_profiles) = overlay_profiles {
                    for (name, profile) in overlay_profiles {
                        merged.insert(name.clone(), profile.clone());
                    }
                }
                Some(merged)
            }
        };

        // Node policy: per-key override — a declaring layer's root/default win;
        // by_type and overrides are replaced wholesale when declared, inherited
        // when omitted. §FS-rhei-states.12.2
        let node_policy = match (&base.node_policy, &overlay.node_policy) {
            (None, None) => None,
            (Some(base_policy), None) => Some(base_policy.clone()),
            (None, Some(overlay_policy)) => Some(overlay_policy.clone()),
            (Some(base_policy), Some(overlay_policy)) => Some(NodePolicy {
                root: overlay_policy.root.clone(),
                default: overlay_policy.default.clone(),
                by_type: if overlay_policy.by_type.is_empty() {
                    base_policy.by_type.clone()
                } else {
                    overlay_policy.by_type.clone()
                },
                overrides: if overlay_policy.overrides.is_empty() {
                    base_policy.overrides.clone()
                } else {
                    overlay_policy.overrides.clone()
                },
            }),
        };

        // Models: union, de-duplicated, lower-layer order first.
        let mut models = base.models.clone();
        for model in &overlay.models {
            if !models.contains(model) {
                models.push(model.clone());
            }
        }

        Ok(StateMachine {
            name: overlay.name.clone(),
            extends: None,
            remove: Vec::new(),
            models,
            version: overlay.version.clone(),
            states,
            transitions,
            profiles,
            node_policy,
        })
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

    // §FS-rhei-states.9.2: Resolve non-root node profiles by policy order.

    /// Resolve the profile for a non-root node, following node-policy order:
    /// `overrides`, `by_type[<kind>]`, then `default`.
    /// Returns `None` when `profiles` / `node_policy` is absent.
    pub fn profile_for_node(&self, kind: &str, level: u8) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        let resolved_name = policy
            .overrides
            .iter()
            .find(|ov| ov.match_.matches(kind, level))
            .map(|ov| ov.profile.as_str())
            .or_else(|| {
                policy
                    .by_type
                    .iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(kind))
                    .map(|(_, profile)| profile.as_str())
            })
            .unwrap_or(policy.default.as_str());
        profiles.get(resolved_name)
    }

    /// §FS-rhei-states.9.2: Resolve the profile bound to the plan-root node.
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
                let mut seen_target_slugs: HashMap<String, String> = HashMap::new();
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
                    let slug = parsed.slug();
                    if let Some(previous) = seen_target_slugs.insert(slug.clone(), selector.clone())
                    {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' has all_targets entries '{previous}' and '{selector}' that normalize to the same snapshot target slug '{slug}'"
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


}

fn reject_explicit_empty_all_targets(yaml: &str) -> Result<(), StateMachineLoadError> {
    // `all_targets` carries `#[serde(default)]`, so serde collapses missing
    // and explicit-empty into the same empty Vec. Re-parse the raw YAML to
    // distinguish them and reject `all_targets: []` as authoring sugar that
    // most likely means "I intended to list targets here and forgot."
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let Some(states) = raw.get("states").and_then(serde_yaml::Value::as_mapping) else {
        return Ok(());
    };

    for (state_name, state_value) in states {
        let Some(state) = state_value.as_mapping() else { continue };
        let Some(all_targets) = state.get("all_targets") else { continue };
        if all_targets.as_sequence().is_some_and(Vec::is_empty) {
            let label = state_name.as_str().unwrap_or("<unknown>");
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{label}' declares 'all_targets: []' but all_targets must be a non-empty list when present"
            )));
        }
    }

    Ok(())
}
