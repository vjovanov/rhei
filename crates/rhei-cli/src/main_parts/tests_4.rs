  while IFS= read -r line || [ -n "$line" ]; do
    printf 'stdin:%s\n' "$line"
  done
fi
printf 'partial'
"#,
        )
        .expect("write fake agent");
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
        script
    }

    #[cfg(unix)]
    fn run_fake_agent_profile(
        profile: CustomAgentProfile,
        agent_id: &str,
        prompt: &str,
    ) -> (String, Vec<rhei_tui::RunEvent>) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from(agent_id),
            profile,
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(10),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let status = spawn_and_wait_agent(
            &resolved,
            prompt,
            dir.path(),
            dir.path(),
            None,
            "task-live",
            "pending",
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder.clone(),
        )
        .expect("fake agent runs");

        assert!(status.status.success());
        let log = fs::read_to_string(&log_path).expect("read log");
        let events = recorder.events.lock().expect("events").clone();
        (log, events)
    }

    #[cfg(unix)]
    fn write_sleeping_fake_agent(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join("sleeping-agent");
        fs::write(
            &script,
            r#"#!/usr/bin/env bash
set -euo pipefail
printf 'stdout:before-timeout\n'
sleep 2
"#,
        )
        .expect("write sleeping fake agent");
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
        script
    }

    #[cfg(unix)]
    fn write_inherited_pipe_fake_agent(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join("inherited-pipe-agent");
        fs::write(
            &script,
            r#"#!/usr/bin/env bash
set -euo pipefail
printf 'stdout:before-background\n'
(sleep 2) &
"#,
        )
        .expect("write inherited pipe fake agent");
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
        script
    }

    #[cfg(unix)]
    #[test]
    fn fake_claude_profile_streams_prompt_flag_output() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command: vec![script.display().to_string()],
            prompt_flag: Some("-p".to_string()),
            stdin_prompt: false,
            ..CustomAgentProfile::default()
        };

        let (log, events) = run_fake_agent_profile(profile, "claude-code", "hello claude");

        assert!(log.contains("prompt:hello claude"));
        assert!(log.contains("stderr:warn"));
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "prompt:hello claude"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn fake_codex_profile_streams_stdin_prompt_output() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command: vec![script.display().to_string()],
            stdin_prompt: true,
            ..CustomAgentProfile::default()
        };

        let (log, events) = run_fake_agent_profile(profile, "codex", "hello codex");

        assert!(log.contains("stdin:hello codex"));
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "stdin:hello codex"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn fake_pi_profile_streams_prompt_flag_output() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_fake_agent(dir.path());
        let profile = CustomAgentProfile {
            command: vec![script.display().to_string()],
            prompt_flag: Some("-p".to_string()),
            stdin_prompt: false,
            ..CustomAgentProfile::default()
        };

        let (log, events) = run_fake_agent_profile(profile, "pi", "hello pi");

        assert!(log.contains("prompt:hello pi"));
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "prompt:hello pi"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn fake_agent_timeout_keeps_output_and_writes_footer() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_sleeping_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile {
                command: vec![script.display().to_string()],
                ..CustomAgentProfile::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(1),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };

        let status = spawn_and_wait_agent(
            &resolved,
            "prompt",
            dir.path(),
            dir.path(),
            None,
            "task-timeout",
            "pending",
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder.clone(),
        )
        .expect("timeout returns process status");

        assert!(!status.status.success());
        assert!(status.timed_out, "spawn_and_wait_agent must flag timeouts");
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdout:before-timeout"));
        assert!(log.contains("=== exit ==="));
        assert!(
            log.contains("agent timed out after"),
            "log must contain spec-required timeout marker, got: {log}"
        );
        let events = recorder.events.lock().expect("events");
        assert!(events.iter().any(|event| matches!(
            event,
            rhei_tui::RunEvent::AgentOutput {
                stream: rhei_tui::AgentStream::Stdout,
                line,
                ..
            } if line == "stdout:before-timeout"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn inherited_output_pipe_does_not_block_agent_completion() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_inherited_pipe_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile {
                command: vec![script.display().to_string()],
                ..CustomAgentProfile::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(10),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };

        let start = Instant::now();
        let status = spawn_and_wait_agent(
            &resolved,
            "prompt",
            dir.path(),
            dir.path(),
            None,
            "task-pipe",
            "pending",
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder,
        )
        .expect("agent should complete without waiting for inherited pipe EOF");

        assert!(status.status.success());
        assert!(start.elapsed() < Duration::from_secs(1), "spawn waited for inherited pipe EOF");
        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.contains("stdout:before-background"));
        assert!(log.contains("=== exit ==="));
    }

    // ---------------------------------------------------------------------
    // Closing gaps from completeness audit (impl-rhei-agents).
    //
    // These tests pin the new validation, command-building, and runtime
    // behavior added when the audit gaps for
    // `docs/functional-spec/rhei-agents.spec.md` were closed.
    // ---------------------------------------------------------------------

    fn machine_with_states(yaml: &str) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(yaml).expect("valid state machine")
    }

    #[test]
    fn validates_agent_command_required_and_mcp_flag_xor() {
        let mut agents = built_in_agents();
        agents.insert(
            "broken".to_string(),
            CustomAgentProfile {
                command: Vec::new(),
                mcp_flag: Some("--mcp".to_string()),
                mcp_config_flag: Some("--mcp-config".to_string()),
                ..Default::default()
            },
        );
        let settings = RheiSettings { agents, ..default_settings() };
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("'broken' has an empty 'command'")),
            "missing 'command' must error: {errs:?}"
        );
        assert!(
            errs.iter().any(|e| e.contains("mutually exclusive")),
            "mcp_flag/mcp_config_flag XOR must error: {errs:?}"
        );
    }

    #[test]
    fn validates_mcp_server_xor_and_url_requires_transport() {
        let mut settings = default_settings();
        settings.mcp_servers.insert(
            "both".to_string(),
            McpServerProfile {
                command: Some(vec!["x".to_string()]),
                url: Some("https://example".to_string()),
                ..Default::default()
            },
        );
        settings.mcp_servers.insert("neither".to_string(), McpServerProfile::default());
        settings.mcp_servers.insert(
            "url-only".to_string(),
            McpServerProfile { url: Some("https://example".to_string()), ..Default::default() },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(errs.iter().any(|e| e.contains("'both' declares both 'command' and 'url'")));
        assert!(errs.iter().any(|e| e.contains("'neither' must declare exactly one")));
        assert!(errs
            .iter()
            .any(|e| e.contains("'url-only' uses 'url' but does not declare 'transport'")));
    }

    #[test]
    fn validates_models_require_provider_and_model() {
        let mut settings = default_settings();
        settings.models.insert("bad".to_string(), ModelProfile::default());
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(errs.iter().any(|e| e.contains("'bad' is missing required field 'provider'")));
        assert!(errs.iter().any(|e| e.contains("'bad' is missing required field 'model'")));
    }

    #[test]
    fn validates_snapshot_operations_require_target_and_session_profile() {
        let settings = default_settings();
        let no_target = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\n",
        );
        let errs = validate_machine_settings_references(&no_target, &settings);
        assert!(
            errs.iter().any(|e| e.contains("snapshot-requires-target")),
            "snapshot operations without an effective target must error: {errs:?}"
        );

        let no_layout = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    target: claude-code:anthropic:model\n    snapshot:\n      emit:\n        name: build\n  done:\n    description: terminal\n    final: true\n",
        );
        let errs = validate_machine_settings_references(&no_layout, &settings);
        assert!(
            errs.iter().any(|e| e.contains("unsupported-snapshot-session")),
            "named emit with no session layout must error: {errs:?}"
        );
    }

    #[test]
    fn validates_required_snapshot_inherit_requires_preload_strategy() {
        let mut settings = default_settings();
        settings.agents.insert(
            "fake".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                session: Some(serde_json::json!({
                    "layout": { "kind": "FlatById", "ext": "jsonl" },
                    "resume": "none"
                })),
                ..Default::default()
            },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  source:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      emit:\n        name: build\n  pending:\n    description: x\n    target: fake:openai:model\n    snapshot:\n      inherit:\n        name: build\n        required: true\n        select:\n          state: source\n  done:\n    description: terminal\n    final: true\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("no supported snapshot preload strategy")),
            "required inherit with ResumeStrategy::None must error: {errs:?}"
        );
    }

    #[test]
    fn validates_agent_mode_forbidden_on_modeless_agent() {
        let mut settings = default_settings();
        settings.agents.insert(
            "noop".to_string(),
            CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
        );
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    agent: noop\n    agent_mode: yolo\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let errs = validate_machine_settings_references(&machine, &settings);
        assert!(
            errs.iter().any(|e| e.contains("'noop' declares no modes")),
            "agent_mode on modeless agent must error: {errs:?}"
        );
    }

    #[test]
    fn appends_mcp_flag_per_resolved_server() {
        let profile = built_in_agents().remove("codex").expect("codex");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile,
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling {
            mcp_servers: vec![
                ResolvedMcpEntry {
                    id: "linear".to_string(),
                    optional: false,
                    definition: Some(McpServerProfile::default()),
                },
                ResolvedMcpEntry {
                    id: "postgres".to_string(),
                    optional: false,
                    definition: Some(McpServerProfile::default()),
                },
            ],
            skills: Vec::new(),
        };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do",
            Path::new("/tmp"),
            Path::new("/tmp"),
            None,
            "task-1",
            "pending",
            &tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let mcp_idxs: Vec<usize> =
            args.iter().enumerate().filter(|(_, a)| *a == "--mcp").map(|(i, _)| i).collect();
        assert_eq!(mcp_idxs.len(), 2, "one --mcp per resolved server: {args:?}");
