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
include!("lib_parts/preamble.rs");
include!("lib_parts/state_defs.rs");
include!("lib_parts/state_machine_impl.rs");
include!("lib_parts/state_machine_snapshots.rs");
include!("lib_parts/state_machine_runtime_validation.rs");
include!("lib_parts/state_machine_profiles.rs");
include!("lib_parts/validation_helpers_1.rs");
include!("lib_parts/validator_entry.rs");
include!("lib_parts/validator_links.rs");

#[cfg(test)]
mod tests {
    include!("lib_parts/tests_1.rs");
    include!("lib_parts/tests_2.rs");
    include!("lib_parts/tests_3.rs");
    include!("lib_parts/tests_profiles.rs");
    include!("lib_parts/tests_poll.rs");
    include!("lib_parts/tests_snapshots.rs");
}
