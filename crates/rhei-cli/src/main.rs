// The CLI is split into include parts to keep source files bounded by §AR-source-file-size.
include!("main_parts/cli_1.rs");
include!("main_parts/cli_2.rs");
include!("main_parts/completions_1.rs");
include!("main_parts/completions_2.rs");

mod templates {
    include!("main_parts/templates_1.rs");
    include!("main_parts/templates_2.rs");
    include!("main_parts/templates_3.rs");
    include!("main_parts/templates_4.rs");
}

include!("main_parts/states_render.rs");
include!("main_parts/metadata_1.rs");
include!("main_parts/metadata_2.rs");
include!("main_parts/transition_1.rs");
include!("main_parts/artifacts.rs");
include!("main_parts/transition_checks.rs");
include!("main_parts/system_transitions_1.rs");
include!("main_parts/system_transitions_2.rs");
include!("main_parts/run_options.rs");
include!("main_parts/settings_1.rs");
include!("main_parts/settings_2.rs");
include!("main_parts/tooling_1.rs");
include!("main_parts/agent_resolution.rs");
include!("main_parts/run_helpers.rs");
include!("main_parts/agent_command.rs");
include!("main_parts/agent_spawn.rs");
include!("main_parts/programs.rs");
include!("main_parts/snapshots_1.rs");
include!("main_parts/snapshots_2.rs");
include!("main_parts/snapshots_3.rs");
include!("main_parts/snapshots_4.rs");
include!("main_parts/run_command.rs");
include!("main_parts/run_agent_mode.rs");
include!("main_parts/run_callback_mode.rs");
include!("main_parts/run_failure_transitions.rs");
include!("main_parts/ready_transition.rs");
include!("main_parts/snapshot_runtime_1.rs");
include!("main_parts/snapshot_runtime_2.rs");
include!("main_parts/next_command.rs");
include!("main_parts/complete_reset_1.rs");
include!("main_parts/complete_reset_2.rs");
include!("main_parts/render_install_1.rs");
include!("main_parts/install_skills_2.rs");
include!("main_parts/diagnostics.rs");

#[cfg(test)]
mod tests {
    include!("main_parts/tests_1.rs");
    include!("main_parts/tests_2.rs");
    include!("main_parts/tests_3.rs");
    include!("main_parts/tests_4.rs");
    include!("main_parts/tests_5.rs");
    include!("main_parts/tests_6.rs");
    include!("main_parts/tests_7.rs");
}
