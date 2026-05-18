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

// The validator is split into include parts to keep source files bounded by §AR-source-file-size.
include!("validator/preamble.rs");
include!("validator/state_defs.rs");
include!("validator/state_machine_impl.rs");
include!("validator/state_machine_snapshots.rs");
include!("validator/state_machine_runtime_validation.rs");
include!("validator/state_machine_profiles.rs");
include!("validator/validation_helpers.rs");
include!("validator/validator_entry.rs");
include!("validator/validator_links.rs");

#[cfg(test)]
mod tests {
    include!("validator/tests_state_machine.rs");
    include!("validator/tests_plan_validation.rs");
    include!("validator/tests_links_tooling.rs");
    include!("validator/tests_profiles.rs");
    include!("validator/tests_poll.rs");
    include!("validator/tests_snapshots.rs");
}
