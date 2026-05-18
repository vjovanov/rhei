    #[test]
    fn resolve_legacy_agent_uses_defaults_agent_timeout() {
        let settings = RheiSettings {
            agent: Some(AgentConfig::from("codex")),
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults {
                model: None,
                agent: None,
                agent_mode: None,
                agent_timeout: Some("45m".to_string()),
                program_timeout: None,
                mcp_servers: None,
                skills: None,
            },
            agents: built_in_agents(),
            models: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            skills: BTreeMap::new(),
            snapshots: None,
        };

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent should resolve")
                .expect("agent should exist");

        assert_eq!(resolved.timeout_secs, Some(45 * 60));
    }

    fn default_settings() -> RheiSettings {
        RheiSettings {
            agent: None,
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults::default(),
            agents: built_in_agents(),
            models: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            skills: BTreeMap::new(),
            snapshots: None,
        }
    }

    #[test]
    fn resolve_legacy_agent_pulls_default_agent_from_model_registry() {
        // Per docs/functional-spec/rhei-agents.spec.md §Resolution Order step
        // 5, when no agent is configured at any level the model's
        // `default_agent` is consulted.
        let mut settings = default_settings();
        settings.model = Some("impl-fast".to_string());
        settings.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                default_agent: Some("claude-code".to_string()),
                agents: BTreeMap::new(),
            },
        );

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent should resolve")
                .expect("agent should be selected via models.<id>.default_agent");

        assert_eq!(resolved.agent.id(), "claude-code");
        assert_eq!(resolved.model.as_deref(), Some("impl-fast"));
        assert_eq!(resolved.model_provider.as_deref(), Some("anthropic"));
        assert_eq!(resolved.model_name.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn resolve_legacy_agent_prefers_model_agent_binding_timeout() {
        // `models.<id>.agents.<agent>.timeout` sits between state-level and
        // agent-profile timeouts in the resolution chain.
        let mut settings = default_settings();
        settings.agent = Some(AgentConfig::from("claude-code"));
        settings.model = Some("impl-fast".to_string());
        let mut agents = BTreeMap::new();
        agents.insert(
            "claude-code".to_string(),
            ModelAgentBinding {
                args: Vec::new(),
                autonomous_args: Vec::new(),
                timeout: Some("90m".to_string()),
            },
        );
        settings.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                default_agent: None,
                agents,
            },
        );
        settings.defaults.agent_timeout = Some("30m".to_string());

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent should resolve")
                .expect("agent should exist");

        assert_eq!(resolved.timeout_secs, Some(90 * 60));
    }

    #[test]
    fn target_selector_literal_model_uses_selector_values_when_registry_collides() {
        let mut settings = default_settings();
        let mut agents = BTreeMap::new();
        agents.insert(
            "codex".to_string(),
            ModelAgentBinding {
                timeout: Some("2m".to_string()),
                ..Default::default()
            },
        );
        settings.models.insert(
            "literal-model".to_string(),
            ModelProfile {
                provider: Some("registry-provider".to_string()),
                model: Some("registry-concrete-model".to_string()),
                default_agent: None,
                agents,
            },
        );

        let resolved =
            resolve_target_agent("codex:selector-provider:literal-model", None, &settings)
                .expect("target resolves");

        assert_eq!(resolved.model.as_deref(), Some("literal-model"));
        assert_eq!(resolved.model_provider.as_deref(), Some("selector-provider"));
        assert_eq!(resolved.model_name.as_deref(), Some("literal-model"));
        assert_eq!(resolved.timeout_secs, Some(120));
    }

    #[test]
    fn mode_default_order_uses_declaration_order() {
        let settings: RheiSettings = serde_json::from_str(
            r#"{
              "defaults": { "agent": "custom" },
              "agents": {
                "custom": {
                  "command": ["custom-agent"],
                  "modes": {
                    "yolo": ["--yolo"],
                    "safe": ["--safe"]
                  }
                }
              }
            }"#,
        )
        .expect("settings parse");

        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent resolves")
                .expect("agent exists");

        assert_eq!(resolved.mode.as_deref(), Some("yolo"));
    }

    #[test]
    fn build_agent_command_uses_concrete_model_name_for_flag() {
        // The `--model` flag should receive the registry-resolved concrete
        // model name (`claude-sonnet-4-6`), not the rhei profile id
        // (`impl-fast`).
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/tmp/workspace"),
            Path::new("/tmp/workspace"),
            None,
            "task-1",
            "pending",
            &tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();

        let model_idx =
            args.iter().position(|arg| arg == "--model").expect("--model flag should be present");
        assert_eq!(args.get(model_idx + 1).map(String::as_str), Some("claude-sonnet-4-6"));
        assert!(!args.iter().any(|arg| arg == "impl-fast"));

        let envs: BTreeMap<String, String> = command
            .get_envs()
            .filter_map(|(k, v)| {
                let key = k.to_string_lossy().into_owned();
                v.map(|val| (key, val.to_string_lossy().into_owned()))
            })
            .collect();
        assert_eq!(envs.get("RHEI_MODEL").map(String::as_str), Some("impl-fast"));
        assert_eq!(envs.get("RHEI_MODEL_PROVIDER").map(String::as_str), Some("anthropic"));
        assert_eq!(envs.get("RHEI_MODEL_NAME").map(String::as_str), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn build_agent_command_falls_back_to_model_id_when_registry_missing() {
        // Backward-compatible behavior: an unregistered model id is passed
        // through as the concrete model name.
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: None,
            target: None,
            model: Some("gpt-5".to_string()),
            model_provider: None,
            model_name: Some("gpt-5".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do work",
            Path::new("/tmp/workspace"),
            Path::new("/tmp/workspace"),
            None,
            "task-1",
            "pending",
            &tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        let model_idx = args.iter().position(|arg| arg == "--model").expect("--model");
        assert_eq!(args.get(model_idx + 1).map(String::as_str), Some("gpt-5"));
    }

    #[test]
    fn settings_parse_models_registry() {
        let json = r#"{
          "models": {
            "impl-fast": {
              "provider": "anthropic",
              "model": "claude-sonnet-4-6",
              "default_agent": "claude-code",
              "agents": {
                "claude-code": {
                  "args": ["--permission-mode", "default"],
                  "autonomous_args": ["--permission-mode", "bypassPermissions"],
                  "timeout": "1h"
                }
              }
            }
          }
        }"#;
        let parsed: RheiSettings = serde_json::from_str(json).expect("parse settings");
        let model = parsed.models.get("impl-fast").expect("impl-fast model");
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert_eq!(model.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(model.default_agent.as_deref(), Some("claude-code"));
        let binding = model.agents.get("claude-code").expect("claude-code binding");
        assert_eq!(binding.timeout.as_deref(), Some("1h"));
    }

    #[test]
    fn format_iso8601_utc_renders_epoch_origin() {
        let epoch = std::time::UNIX_EPOCH;
        assert_eq!(format_iso8601_utc(epoch), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_iso8601_utc_renders_known_instant() {
        // 2026-04-20T10:30:00Z = 1_776_681_000 seconds since epoch.
        let when = std::time::UNIX_EPOCH + Duration::from_secs(1_776_681_000);
        assert_eq!(format_iso8601_utc(when), "2026-04-20T10:30:00Z");
    }

    #[test]
    fn format_duration_human_matches_spec_examples() {
        assert_eq!(format_duration_human(0), "0s");
        assert_eq!(format_duration_human(30), "30s");
        assert_eq!(format_duration_human(5 * 60), "5m");
        assert_eq!(format_duration_human(60 * 60), "1h");
        assert_eq!(format_duration_human(2 * 3600 + 30 * 60), "2h30m");
        assert_eq!(format_duration_human(4 * 60 + 23), "4m23s");
    }

    #[cfg(unix)]
    #[test]
    fn agent_log_header_uses_v1_format_and_spec_fields() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_quiet_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile: CustomAgentProfile {
                command: vec![script.display().to_string()],
                ..CustomAgentProfile::default()
            },
            mode: Some("yolo".to_string()),
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: Some("anthropic".to_string()),
            model_name: Some("claude-sonnet-4-6".to_string()),
            timeout_secs: Some(1800),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };

        spawn_and_wait_agent(
            &resolved,
            "prompt",
            dir.path(),
            dir.path(),
            None,
            "task-log",
            "pending",
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder,
        )
        .expect("agent runs");

        let log = fs::read_to_string(&log_path).expect("read log");
        assert!(log.starts_with("=== rhei agent log v1 ==="), "header missing v1: {log}");
        assert!(log.contains("\nprovider: anthropic\n"));
        assert!(log.contains("\nmodel_name: claude-sonnet-4-6\n"));
        assert!(log.contains("\ntimeout: 30m\n"));
        // started/ended ISO timestamps and human-readable duration.
        assert!(log.contains("\nstarted: "));
        assert!(log.contains("\nended: "));
        assert!(log.contains("\nduration: "));
        // No legacy "1800s"-style numeric timeout anywhere.
        assert!(!log.contains("\ntimeout: 1800s\n"));
    }

    #[cfg(unix)]
    fn write_quiet_fake_agent(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("quiet-agent.sh");
        fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write script");
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("set perms");
        script
    }

    #[test]
    fn built_in_codex_yolo_includes_approval_never() {
        // docs/functional-spec/rhei-agents.spec.md §Known Agent Profiles
        // requires the codex yolo mode to include `-a never` so the agent
        // never prompts for approval interactively.
        let profile = built_in_agents().remove("codex").expect("built-in codex");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile,
            mode: Some("yolo".to_string()),
            target: Some(parse_execution_target("codex[yolo]:openai:gpt-5-codex").expect("target")),
            model: Some("gpt-5-codex".to_string()),
            model_provider: Some("openai".to_string()),
            model_name: Some("gpt-5-codex".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "analyze this",
            Path::new("/tmp/workspace"),
            Path::new("/tmp/workspace"),
            None,
            "analysis",
            "analyze",
            &tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();

        assert!(args.windows(2).any(|pair| pair == ["--sandbox", "danger-full-access"]));
        assert!(args.iter().any(|arg| arg == "--skip-git-repo-check"));
        assert!(args.windows(2).any(|pair| pair == ["-a", "never"]));
    }

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<rhei_tui::RunEvent>>,
    }

    impl rhei_tui::EventSink for RecordingSink {
        fn emit(&self, event: rhei_tui::RunEvent) {
            self.events.lock().expect("recording sink lock").push(event);
        }
    }

    #[test]
    fn output_reader_logs_and_emits_complete_and_partial_lines() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let log_path = dir.path().join("agent.log");
        let log_file = Arc::new(Mutex::new(fs::File::create(&log_path).expect("log file")));
        let recorder = Arc::new(RecordingSink::default());
        let sink: Arc<dyn rhei_tui::EventSink> = recorder.clone();

        let handle = spawn_agent_output_reader(
            std::io::Cursor::new(b"first\npartial".to_vec()),
            rhei_tui::AgentStream::Stdout,
            log_file,
            sink,
            3,
            "task-live".to_string(),
        );

        drain_agent_output_reader(handle, rhei_tui::AgentStream::Stdout).expect("reader drains");

        let log = fs::read_to_string(&log_path).expect("read log");
        assert_eq!(log, "first\npartial");

        let events = recorder.events.lock().expect("events");
        assert_eq!(events.len(), 2);
        match &events[0] {
            rhei_tui::RunEvent::AgentOutput { slot, task, stream, line, .. } => {
                assert_eq!(*slot, 3);
                assert_eq!(task, "task-live");
                assert_eq!(*stream, rhei_tui::AgentStream::Stdout);
                assert_eq!(line, "first");
            }
            other => panic!("expected AgentOutput, got {other:?}"),
        }
        match &events[1] {
            rhei_tui::RunEvent::AgentOutput { line, .. } => assert_eq!(line, "partial"),
            other => panic!("expected AgentOutput, got {other:?}"),
        }
    }

    #[test]
    fn supported_agents_keep_expected_prompt_transports() {
        let agents = built_in_agents();
        let claude = agents.get("claude-code").expect("claude-code profile");
        assert_eq!(claude.prompt_flag.as_deref(), Some("-p"));
        assert!(!claude.stdin_prompt);

        let codex = agents.get("codex").expect("codex profile");
        assert_eq!(codex.prompt_flag.as_deref(), None);
        assert!(codex.stdin_prompt);

        let pi = agents.get("pi").expect("pi profile");
        assert_eq!(pi.prompt_flag.as_deref(), Some("-p"));
        assert!(!pi.stdin_prompt);
    }

    #[cfg(unix)]
    fn write_fake_agent(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join("fake-agent");
        fs::write(
            &script,
            r#"#!/usr/bin/env bash
set -euo pipefail
printf 'stdout:start\n'
printf 'stderr:warn\n' >&2
prev=''
read_stdin=0
for arg in "$@"; do
  if [ "$prev" = "-p" ]; then
    printf 'prompt:%s\n' "$arg"
  fi
  if [ "$arg" = "--" ]; then
    read_stdin=1
  fi
  prev="$arg"
done
if [ "$read_stdin" = "1" ]; then
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
