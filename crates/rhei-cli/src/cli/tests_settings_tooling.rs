    #[test]
    fn appends_mcp_config_flag_with_temp_file() {
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
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
            mcp_servers: vec![ResolvedMcpEntry {
                id: "linear".to_string(),
                optional: false,
                definition: Some(McpServerProfile {
                    command: Some(vec!["mcp-linear".to_string()]),
                    ..Default::default()
                }),
            }],
            skills: Vec::new(),
        };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do",
            Path::new("/tmp"),
            Path::new("/tmp"),
            None,
            "task-7",
            "pending",
            &tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let idx = args.iter().position(|a| a == "--mcp-config").expect("--mcp-config emitted");
        let path = PathBuf::from(&args[idx + 1]);
        assert!(path.exists(), "mcp config file '{}' written", path.display());
        let body = fs::read_to_string(&path).expect("read config");
        assert!(body.contains("\"mcpServers\""));
        assert!(body.contains("\"linear\""));
    }

    #[test]
    fn appends_autonomous_args_after_mode_flags() {
        let profile = built_in_agents().remove("claude-code").expect("claude-code");
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile,
            mode: Some("yolo".to_string()),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: vec!["--auto-flag".to_string(), "value".to_string()],
        };
        let tooling = ResolvedTooling { mcp_servers: Vec::new(), skills: Vec::new() };
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "do",
            Path::new("/tmp"),
            Path::new("/tmp"),
            None,
            "t",
            "pending",
            &tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let mode_idx = args.iter().position(|a| a == "bypassPermissions").expect("mode flag");
        let auto_idx = args.iter().position(|a| a == "--auto-flag").expect("autonomous arg");
        assert!(auto_idx > mode_idx, "autonomous_args must follow mode flags: {args:?}");
    }

    #[test]
    fn settings_parses_nested_defaults_and_snapshots_block() {
        let json = r#"{
          "defaults": {
            "model": "impl-fast",
            "agent": "claude-code",
            "agent_mode": "yolo",
            "agent_timeout": "30m",
            "program_timeout": "10m"
          },
          "snapshots": {
            "cache_dir": ".rhei/cache/snapshots"
          }
        }"#;
        let parsed: RheiSettings = serde_json::from_str(json).expect("parse settings");
        assert_eq!(parsed.defaults.model.as_deref(), Some("impl-fast"));
        assert_eq!(parsed.defaults.agent.as_ref().map(|a| a.id()), Some("claude-code"));
        assert_eq!(parsed.defaults.agent_mode.as_deref(), Some("yolo"));
        assert_eq!(parsed.defaults.agent_timeout.as_deref(), Some("30m"));
        assert_eq!(parsed.defaults.program_timeout.as_deref(), Some("10m"));
        assert!(parsed.snapshots.is_some());
    }

    #[test]
    fn expand_env_vars_substitutes_present_ignores_missing() {
        std::env::set_var("RHEI_TEST_VAR", "hello");
        assert_eq!(expand_env_vars("/path/${RHEI_TEST_VAR}/y"), "/path/hello/y");
        // Unknown vars expand to empty string per spec.
        std::env::remove_var("RHEI_TEST_UNKNOWN");
        assert_eq!(expand_env_vars("a${RHEI_TEST_UNKNOWN}b"), "ab");
        // No `${` patterns are passed through.
        assert_eq!(expand_env_vars("plain"), "plain");
    }

    #[test]
    fn resolved_agent_log_suffix_includes_visit_count() {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("claude-code"),
            profile: CustomAgentProfile {
                command: vec!["claude".to_string()],
                ..Default::default()
            },
            mode: None,
            target: None,
            model: Some("impl-fast".to_string()),
            model_provider: None,
            model_name: Some("impl-fast".to_string()),
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        // Visit 1 stays unsuffixed (no counted-loop noise on first visit).
        assert_eq!(resolved_agent_log_suffix(&resolved, Some(1)).as_deref(), Some("impl-fast"));
        // Visits > 1 are appended after the model slug.
        assert_eq!(resolved_agent_log_suffix(&resolved, Some(3)).as_deref(), Some("impl-fast-3"));
        // Without a model, the suffix is just the visit count.
        let resolved_no_model = ResolvedAgent { model: None, model_name: None, ..resolved.clone() };
        assert_eq!(resolved_agent_log_suffix(&resolved_no_model, Some(2)).as_deref(), Some("2"));
    }

    #[test]
    fn collect_unsupported_tooling_warnings_reports_dropped_entries() {
        let profile = CustomAgentProfile { command: vec!["x".to_string()], ..Default::default() };
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("noop"),
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
            mcp_servers: vec![ResolvedMcpEntry {
                id: "linear".to_string(),
                optional: false,
                definition: Some(McpServerProfile::default()),
            }],
            skills: vec![ResolvedSkillEntry {
                id: "test-authoring".to_string(),
                optional: false,
                definition: Some(SkillProfile {
                    path: "/skills/test-authoring".to_string(),
                    description: None,
                }),
            }],
        };
        let warnings = collect_unsupported_tooling_warnings(&resolved, &tooling);
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("no mcp_flag/mcp_config_flag")));
        assert!(warnings.iter().any(|w| w.contains("no skill_flag")));
    }

    #[test]
    fn merge_deep_merges_models_agents_by_binding_id() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan_root = dir.path().join("plan");
        let project_dir = plan_root.join(".rhei");
        fs::create_dir_all(&project_dir).expect("mkdir");
        // Global settings define args; project settings define autonomous_args
        // for the same model-agent binding. Deep-merge must keep both.
        let _global_home = TempHome::new();
        let global_dir = home_dir().expect("home").join(".config/rhei");
        fs::create_dir_all(&global_dir).expect("global dir");
        fs::write(
            global_dir.join("settings.json"),
            r#"{
              "models": {
                "impl-fast": {
                  "provider": "anthropic",
                  "model": "claude-sonnet-4-6",
                  "agents": {
                    "claude-code": { "args": ["--global-arg"] }
                  }
                }
              }
            }"#,
        )
        .expect("write global");
        fs::write(
            project_dir.join("settings.json"),
            r#"{
              "models": {
                "impl-fast": {
                  "agents": {
                    "claude-code": { "autonomous_args": ["--project-arg"] }
                  }
                }
              }
            }"#,
        )
        .expect("write project");
        let merged = load_merged_settings(&plan_root).expect("merge settings");
        let model = merged.models.get("impl-fast").expect("impl-fast survives");
        // Project did not respecify provider/model, so they inherit.
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert_eq!(model.model.as_deref(), Some("claude-sonnet-4-6"));
        let binding = model.agents.get("claude-code").expect("claude-code binding");
        // The project entry replaces the agent binding by id (wholesale per
        // bindings), but the merge is at the model level — both registries
        // observe the deep-merge of the inner `agents` map.
        assert_eq!(binding.autonomous_args, vec!["--project-arg".to_string()]);
    }

    #[test]
    fn settings_missing_file_defaults_and_malformed_project_errors() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let missing = dir.path().join("missing-settings.json");
        let settings = load_settings(&missing).expect("missing settings default");
        assert!(settings.agent.is_none());

        let _home = TempHome::new();
        let plan_root = dir.path().join("plan");
        let project_dir = plan_root.join(".rhei");
        fs::create_dir_all(&project_dir).expect("mkdir");
        let project_settings = project_dir.join("settings.json");
        fs::write(&project_settings, "{ not json").expect("write malformed");

        let err = load_merged_settings(&plan_root).expect_err("malformed project settings fails");
        let msg = err.to_string();
        assert!(msg.contains(project_settings.to_string_lossy().as_ref()), "{msg}");
    }

    #[test]
    fn settings_merge_project_null_clears_inherited_optional_defaults() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan_root = dir.path().join("plan");
        let project_dir = plan_root.join(".rhei");
        fs::create_dir_all(&project_dir).expect("mkdir");
        let _home = TempHome::new();
        let global_dir = home_dir().expect("home").join(".config/rhei");
        fs::create_dir_all(&global_dir).expect("global dir");
        fs::write(
            global_dir.join("settings.json"),
            r#"{
              "agent": "codex",
              "agent_mode": "yolo",
              "model": "impl-fast",
              "defaults": {
                "agent": "codex",
                "agent_mode": "yolo",
                "model": "impl-fast"
              },
              "models": {
                "impl-fast": {
                  "provider": "anthropic",
                  "model": "claude-sonnet-4-6",
                  "default_agent": "codex",
                  "agents": {
                    "codex": { "timeout": "30m" }
                  }
                }
              }
            }"#,
        )
        .expect("write global");
        fs::write(
            project_dir.join("settings.json"),
            r#"{
              "agent": null,
              "agent_mode": null,
              "model": null,
              "defaults": {
                "agent": null,
                "agent_mode": null,
                "model": null
              },
              "models": {
                "impl-fast": {
                  "default_agent": null,
                  "agents": {
                    "codex": { "timeout": null }
                  }
                }
              }
            }"#,
        )
        .expect("write project");

        let merged = load_merged_settings(&plan_root).expect("merge settings");
        assert!(merged.agent.is_none());
        assert!(merged.agent_mode.is_none());
        assert!(merged.model.is_none());
        assert!(merged.defaults.agent.is_none());
        assert!(merged.defaults.agent_mode.is_none());
        assert!(merged.defaults.model.is_none());
        let model = merged.models.get("impl-fast").expect("model inherited");
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert!(model.default_agent.is_none());
        assert!(model.agents.get("codex").expect("binding").timeout.is_none());
    }

    #[test]
    fn model_registry_resolve_legacy_agent_rejects_unknown_configured_model_id() {
        let mut settings = default_settings();
        settings.defaults.model = Some("missing-model".to_string());
        settings.defaults.agent = Some(AgentConfig::from("codex"));

        let err =
            match resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None) {
                Ok(_) => panic!("unknown model id must fail"),
                Err(err) => err,
            };
        assert!(err.to_string().contains("model 'missing-model' is not defined"));

        settings.defaults.model = None;
        let resolved =
            resolve_legacy_agent_with_model(None, &settings, &default_run_options(), None)
                .expect("agent resolves without model registry context")
                .expect("agent exists");
        assert!(resolved.model.is_none());
    }

    #[test]
    fn run_readiness_excludes_assigned_tasks_without_changing_ready_semantics() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Assigned

## Tasks

### Task 1: Claimed
**State:** pending
**Assignee:** codex

### Task 2: Open
**State:** pending
"#,
        )
        .expect("parse plan");
        let machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let dir = tempfile::tempdir().expect("tmpdir");
        let ready = find_ready_tasks(&rhei, &machine, dir.path());
        assert_eq!(ready.len(), 2);
        let runnable = find_runnable_tasks(&rhei, &machine, dir.path());
        assert_eq!(
            runnable.iter().map(|task| task.id.to_string()).collect::<Vec<_>>(),
            vec!["2".to_string()]
        );
    }

    #[test]
    fn defaults_only_agent_mode_selects_agent_mode_for_effective_agents() {
        let bare_machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );
        let model_machine = machine_with_states(
            "name: t\nversion: 1\nstates:\n  pending:\n    description: x\n    model: impl-fast\n  done:\n    description: terminal\n    final: true\ntransitions:\n  - from: pending\n    to: done\n",
        );

        let mut cli_opts = default_run_options();
        cli_opts.agent.agent = Some("codex".to_string());
        assert!(should_use_agent_mode(&bare_machine, &default_settings(), &cli_opts)
            .expect("cli agent mode selection"));

        let mut defaults_agent = default_settings();
        defaults_agent.defaults.agent = Some(AgentConfig::from("codex"));
        assert!(should_use_agent_mode(&bare_machine, &defaults_agent, &default_run_options())
            .expect("defaults.agent mode selection"));

        let mut defaults_model = default_settings();
        defaults_model.defaults.model = Some("impl-fast".to_string());
        defaults_model.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet".to_string()),
                default_agent: Some("codex".to_string()),
                agents: BTreeMap::new(),
            },
        );
        assert!(should_use_agent_mode(&bare_machine, &defaults_model, &default_run_options())
            .expect("defaults.model mode selection"));

        let mut model_default_agent = default_settings();
        model_default_agent.models.insert(
            "impl-fast".to_string(),
            ModelProfile {
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet".to_string()),
                default_agent: Some("codex".to_string()),
                agents: BTreeMap::new(),
            },
        );
        assert!(should_use_agent_mode(&model_machine, &model_default_agent, &default_run_options())
            .expect("models.<id>.default_agent mode selection"));
    }

    #[test]
    fn tooling_gate_classifies_required_optional_and_transition_triggers() {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("noop"),
            profile: CustomAgentProfile { command: vec!["noop".to_string()], ..Default::default() },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "optional-mcp".to_string(),
                optional: true,
                definition: None,
            }],
            skills: vec![ResolvedSkillEntry {
                id: "required-skill".to_string(),
                optional: false,
                definition: Some(SkillProfile {
                    path: "/tmp/skill".to_string(),
                    description: None,
                }),
            }],
        };

        let gate = gate_tooling_for_agent(&resolved, &tooling);
        assert_eq!(gate.tooling.mcp_servers.len(), 1);
        assert_eq!(gate.tooling.mcp_servers[0].id, "optional-mcp");
        assert!(gate.tooling.mcp_servers[0].definition.is_none());
        assert!(gate.tooling.skills.is_empty());
        assert_eq!(gate.warnings.len(), 1);
        assert_eq!(gate.required.len(), 1);
        assert_eq!(gate.required[0].kind, ToolingKind::Skill);
        assert_eq!(gate.required[0].id, "required-skill");

        assert!(tooling_trigger_matches(
            &serde_yaml::Value::Bool(true),
            &["required-skill".to_string()]
        ));
        assert!(tooling_trigger_matches(
            &serde_yaml::from_str("[other, required-skill]").expect("yaml"),
            &["required-skill".to_string()]
        ));
        assert!(!tooling_trigger_matches(
            &serde_yaml::from_str("[other]").expect("yaml"),
            &["required-skill".to_string()]
        ));
    }

    #[cfg(unix)]
    #[test]
    fn optional_tooling_availability_preserves_prompt_env_and_log_visibility() {
        let machine = machine_with_states(
            r#"name: optional-tooling
version: 1
states:
  pending:
    description: x
    instructions: "mcp={mcp.optional-mcp.available} skill={skill.optional-skill.available}"
  done:
    description: terminal
    final: true
"#,
        );
        let rhei = rhei_core::parse(
            r#"# Rhei: Optional Tooling

## Tasks

### Task 1: Work
**State:** pending
"#,
        )
        .expect("parse plan");
        let task = rhei.tasks.first().expect("task");
        let tooling = ResolvedTooling {
            mcp_servers: vec![ResolvedMcpEntry {
                id: "optional-mcp".to_string(),
                optional: true,
                definition: None,
            }],
            skills: vec![ResolvedSkillEntry {
                id: "optional-skill".to_string(),
                optional: true,
                definition: None,
            }],
        };
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: built_in_agents().remove("codex").expect("codex"),
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let gate = gate_tooling_for_agent(&resolved, &tooling);
        assert_eq!(gate.tooling.mcp_servers.len(), 1);
        assert_eq!(gate.tooling.skills.len(), 1);

        let render_context = RuntimeTemplateContext {
            workspace_root: Path::new("/tmp/workspace"),
            plan_path: Path::new("/tmp/workspace/plan.rhei.md"),
            state_machine_path: None,
            plan_title: &rhei.title,
            task,
            state_name: "pending",
            current_state_raw: task.state.as_str(),
            machine: &machine,
            metadata: rhei.metadata.as_ref(),
            target: None,
            model: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: Some(&gate.tooling),
        };
        let prompt = compose_agent_prompt(&render_context);
        assert!(prompt.contains("mcp=false skill=false"), "{prompt}");

        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        let command = build_agent_command(
            &resolved,
            "prompt",
            runtime_dir.path(),
            runtime_dir.path(),
            None,
            "1",
            "pending",
            &gate.tooling,
            runtime_dir.path(),
        );
        let args: Vec<String> =
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        assert!(!args.windows(2).any(|pair| pair == ["--mcp", "optional-mcp"]));
        let envs: BTreeMap<String, String> = command
            .get_envs()
            .filter_map(|(k, v)| {
                let key = k.to_string_lossy().into_owned();
                v.map(|val| (key, val.to_string_lossy().into_owned()))
            })
            .collect();
        assert_eq!(
            envs.get("RHEI_MCP_OPTIONAL_MCP_AVAILABLE").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            envs.get("RHEI_SKILL_OPTIONAL_SKILL_AVAILABLE").map(String::as_str),
            Some("false")
        );

        let script = write_quiet_fake_agent(runtime_dir.path());
        let log_path = runtime_dir.path().join("agent.log");
        let log_resolved = ResolvedAgent {
            profile: CustomAgentProfile {
                command: vec![script.display().to_string()],
                ..Default::default()
            },
            ..resolved
        };
        spawn_and_wait_agent(
            &log_resolved,
            "prompt",
            runtime_dir.path(),
            runtime_dir.path(),
            None,
            "1",
            "pending",
            &gate.tooling,
            &log_path,
            runtime_dir.path(),
            None,
            0,
            Arc::new(RecordingSink::default()),
        )
        .expect("agent runs");
        let log = fs::read_to_string(log_path).expect("read log");
        assert!(log.contains("\nmcp_servers: optional-mcp?\n"), "{log}");
        assert!(log.contains("\nskills: optional-skill?\n"), "{log}");
    }

    #[test]
    fn tooling_required_missing_skill_blocks_fake_agent_spawn() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        let rhei_dir = dir.path().join(".rhei");
        fs::create_dir_all(&rhei_dir).expect("mkdir");
        let spawned = dir.path().join("spawned");
        let script = dir.path().join("fake-agent.sh");
        fs::write(&script, format!("#!/bin/sh\ntouch '{}'\n", spawned.display())).expect("script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod");
        }
        fs::write(
            &plan,
            r#"# Rhei: Missing Skill

## Tasks

### Task 1: Work
**State:** pending
"#,
        )
        .expect("plan");
        fs::write(
            &states,
            r#"name: missing-skill
version: 1
states:
  pending:
    description: pending
    agent: fake
    agent_timeout: 1s
    skills:
      - missing
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("states");
        fs::write(
            rhei_dir.join("settings.json"),
            format!(
                r#"{{
                  "agents": {{
                    "fake": {{
                      "command": [{}],
                      "prompt_flag": "--prompt"
                    }}
                  }},
                  "skills": {{
                    "missing": {{ "path": "{}" }}
                  }}
                }}"#,
                serde_json::to_string(script.to_string_lossy().as_ref()).expect("json"),
                dir.path().join("does-not-exist").display()
            ),
        )
        .expect("settings");

        let mut opts = default_run_options();
        opts.standalone.continue_on_error = true;
        opts.standalone.no_tui = true;
        run_command(&plan, Some(&states), opts).expect("run skips unavailable tooling");

        assert!(!spawned.exists(), "required missing skill must block agent spawn");
    }
