//! Rhei Output
//!
//! Output generators for rhei plans. Currently ships JSON, GitHub-issues
//! markdown, and a terminal progress report. All generators walk the
//! recursive task tree produced by [`rhei_core::ast`].

/// Returns this crate's version.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Trait for output generators.
pub trait OutputGenerator {
    fn generate(&self, input: &str) -> String;
}

/// A no-op output generator that returns the input unchanged.
#[derive(Debug, Default, Clone)]
pub struct NoopOutput;

impl OutputGenerator for NoopOutput {
    fn generate(&self, input: &str) -> String {
        input.to_string()
    }
}

mod common;
mod github;
mod json;
mod progress;

pub use github::{to_github_markdown, GithubIssuesOutput};
pub use json::{to_json_string_pretty, to_json_value, JsonOutput};
pub use progress::{to_progress_report, ProgressReportOutput};

/// Plan output generator trait for structured outputs.
pub trait PlanOutputGenerator {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value;
}

#[cfg(test)]
mod tests;
