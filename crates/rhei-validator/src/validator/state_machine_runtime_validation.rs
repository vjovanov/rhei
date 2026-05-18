impl StateMachine {
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
            if state.snapshot.as_ref().and_then(|snapshot| snapshot.inherit.as_ref()).is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares both 'poll' and 'snapshot.inherit'; polling states cannot inherit snapshots in v1"
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
}
