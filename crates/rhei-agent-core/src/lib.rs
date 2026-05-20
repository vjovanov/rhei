//! Embeddable agent runtime primitives for Rhei workflows.
//!
//! This crate is the public home for runtime-facing APIs. In the 0.1 line it
//! starts as a small facade over the shared plan model in `rhei-plan-core`, so
//! downstream users can depend on the stable runtime crate name while execution
//! internals continue to move out of the CLI.

pub use rhei_core::{ast, callback, lexer, parser, tokens, workspace};
pub use rhei_core::{parse, tokenize, Tokenizer};

/// Returns the `rhei-agent-core` crate version reported by Cargo metadata.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the `rhei-plan-core` crate version this runtime facade is built on.
pub fn plan_core_version() -> String {
    rhei_core::version()
}

/// Human-readable description of the runtime surface.
pub fn description() -> &'static str {
    "Embeddable agent runtime primitives for governed Rhei workflows"
}
