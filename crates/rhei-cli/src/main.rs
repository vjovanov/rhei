// §AR-source-file-size: The CLI is split into bounded include parts.
include!("cli/cli_declarations.rs");
include!("cli/cli_dispatch.rs");
include!("cli/completion_candidates.rs");
include!("cli/completion_context.rs");

mod templates {
    include!("cli/templates_list.rs");
    include!("cli/templates_instantiate.rs");
    include!("cli/templates_discovery.rs");
    include!("cli/templates_inputs.rs");
}

include!("cli/states_render.rs");
include!("cli/metadata_conditions.rs");
include!("cli/metadata_rewrite.rs");
include!("cli/transition_context.rs");
include!("cli/artifacts.rs");
include!("cli/transition_checks.rs");
include!("cli/system_transition_triggers.rs");
include!("cli/system_transition_execution.rs");
include!("cli/run_options.rs");
include!("cli/settings_types.rs");
include!("cli/settings_load_validate.rs");
include!("cli/tooling_resolution.rs");
include!("cli/agent_resolution.rs");
include!("cli/run_helpers.rs");
include!("cli/agent_command.rs");
include!("cli/agent_spawn.rs");
include!("cli/intervene.rs");
include!("cli/accounting.rs");
include!("cli/programs.rs");
include!("cli/snapshot_records.rs");
include!("cli/snapshot_list_show.rs");
include!("cli/snapshot_refs_gc.rs");
include!("cli/snapshot_continue_lock.rs");
include!("cli/run_command.rs");
include!("cli/run_agent_mode.rs");
include!("cli/run_callback_mode.rs");
include!("cli/run_failure_transitions.rs");
include!("cli/ready_transition.rs");
include!("cli/snapshot_runtime_emit.rs");
include!("cli/snapshot_runtime_preload.rs");
include!("cli/next_command.rs");
include!("cli/complete_reset_commands.rs");
include!("cli/complete_reset_rewrites.rs");
include!("cli/render_install_commands.rs");
include!("cli/install_skill_agents.rs");
include!("cli/viz_command.rs");
include!("cli/intervene_command.rs");
include!("cli/diagnostics.rs");

#[cfg(test)]
mod tests {
    include!("cli/tests_cli_render.rs");
    include!("cli/tests_complete_reset_tooling.rs");
    include!("cli/tests_agent_resolution.rs");
    include!("cli/tests_agent_execution_validation.rs");
    include!("cli/tests_accounting.rs");
    include!("cli/tests_settings_tooling.rs");
    include!("cli/tests_snapshots_gc.rs");
    include!("cli/tests_snapshot_runtime.rs");
}
