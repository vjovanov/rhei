//! Callback execution interface for state transition rules.
//!
//! When a transition rule declares `on_leave` or `on_enter` callbacks, a
//! [`CallbackExecutor`] is responsible for resolving and invoking them.
//!
//! The [`ShellCallbackExecutor`] implementation runs callbacks as shell
//! commands, passing transition context via environment variables.

use crate::ast::CallbackRef;
use std::path::Path;

/// Context provided to a callback during a state transition.
#[derive(Debug, Clone)]
pub struct CallbackContext<'a> {
    /// Identifier of the task being transitioned.
    pub task_id: &'a str,
    /// State the task is leaving.
    pub from_state: &'a str,
    /// State the task is entering.
    pub to_state: &'a str,
    /// Path to the plan file.
    pub plan_path: &'a Path,
}

/// Outcome of a callback invocation.
#[derive(Debug, Clone)]
pub struct CallbackResult {
    /// Whether the callback succeeded.
    pub success: bool,
    /// Stdout captured from the callback (if any).
    pub stdout: String,
    /// Stderr captured from the callback (if any).
    pub stderr: String,
}

/// Error returned when a callback cannot be executed.
#[derive(Debug)]
pub enum CallbackError {
    /// The callback identifier has no recognized platform prefix.
    UnknownPlatform(String),
    /// The callback command could not be started.
    SpawnFailed(String, std::io::Error),
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallbackError::UnknownPlatform(id) => {
                write!(f, "unknown callback platform in '{id}' (expected prefix like 'cli:')")
            }
            CallbackError::SpawnFailed(cmd, err) => {
                write!(f, "failed to execute callback '{cmd}': {err}")
            }
        }
    }
}

impl std::error::Error for CallbackError {}

/// Trait for executing transition callbacks.
///
/// Implementations resolve a [`CallbackRef`] identifier and invoke the
/// corresponding callback, passing the [`CallbackContext`] as input.
pub trait CallbackExecutor {
    /// Execute a callback and return its result.
    fn execute(
        &self,
        callback: &CallbackRef,
        context: &CallbackContext<'_>,
    ) -> Result<CallbackResult, CallbackError>;
}

/// Executes `cli:`-prefixed callbacks as shell commands.
///
/// The command portion after `cli:` is run via `sh -c`, with transition
/// context exported as environment variables:
///
/// - `RHEI_TASK_ID` — the task identifier
/// - `RHEI_FROM_STATE` — the state being left
/// - `RHEI_TO_STATE` — the state being entered
/// - `RHEI_PLAN_PATH` — path to the plan file
///
/// A zero exit code means success; non-zero means failure.
pub struct ShellCallbackExecutor;

impl CallbackExecutor for ShellCallbackExecutor {
    fn execute(
        &self,
        callback: &CallbackRef,
        context: &CallbackContext<'_>,
    ) -> Result<CallbackResult, CallbackError> {
        let id = &callback.0;

        let command =
            id.strip_prefix("cli:").ok_or_else(|| CallbackError::UnknownPlatform(id.clone()))?;

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("RHEI_TASK_ID", context.task_id)
            .env("RHEI_FROM_STATE", context.from_state)
            .env("RHEI_TO_STATE", context.to_state)
            .env("RHEI_PLAN_PATH", context.plan_path.as_os_str())
            .output()
            .map_err(|e| CallbackError::SpawnFailed(command.to_string(), e))?;

        Ok(CallbackResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// A no-op executor that skips all callbacks (used with `--no-callbacks`).
pub struct NoopCallbackExecutor;

impl CallbackExecutor for NoopCallbackExecutor {
    fn execute(
        &self,
        _callback: &CallbackRef,
        _context: &CallbackContext<'_>,
    ) -> Result<CallbackResult, CallbackError> {
        Ok(CallbackResult { success: true, stdout: String::new(), stderr: String::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_executor_rejects_non_cli_prefix() {
        let executor = ShellCallbackExecutor;
        let callback = CallbackRef("js:someFunction".to_string());
        let context = CallbackContext {
            task_id: "1",
            from_state: "pending",
            to_state: "in-progress",
            plan_path: Path::new("plan.rhei.md"),
        };

        let err = executor.execute(&callback, &context).unwrap_err();
        assert!(matches!(err, CallbackError::UnknownPlatform(_)));
        assert!(err.to_string().contains("js:someFunction"));
    }

    #[test]
    fn shell_executor_runs_successful_command() {
        let executor = ShellCallbackExecutor;
        let callback = CallbackRef("cli:echo hello".to_string());
        let context = CallbackContext {
            task_id: "1",
            from_state: "pending",
            to_state: "in-progress",
            plan_path: Path::new("plan.rhei.md"),
        };

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[test]
    fn shell_executor_reports_failure_on_nonzero_exit() {
        let executor = ShellCallbackExecutor;
        let callback = CallbackRef("cli:exit 1".to_string());
        let context = CallbackContext {
            task_id: "1",
            from_state: "pending",
            to_state: "in-progress",
            plan_path: Path::new("plan.rhei.md"),
        };

        let result = executor.execute(&callback, &context).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn shell_executor_passes_env_vars() {
        let executor = ShellCallbackExecutor;
        let callback =
            CallbackRef("cli:echo $RHEI_TASK_ID $RHEI_FROM_STATE $RHEI_TO_STATE".to_string());
        let context = CallbackContext {
            task_id: "42",
            from_state: "pending",
            to_state: "in-progress",
            plan_path: Path::new("my-plan.rhei.md"),
        };

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout.trim(), "42 pending in-progress");
    }

    #[test]
    fn noop_executor_always_succeeds() {
        let executor = NoopCallbackExecutor;
        let callback = CallbackRef("cli:anything".to_string());
        let context = CallbackContext {
            task_id: "1",
            from_state: "pending",
            to_state: "in-progress",
            plan_path: Path::new("plan.rhei.md"),
        };

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
    }
}
