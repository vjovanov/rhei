    #[test]
    fn paths_equivalent_falls_back_for_nonexistent_relative_paths() {
        assert!(paths_equivalent(
            Path::new("./docs/markdown-plan-compiler.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
        assert!(!paths_equivalent(
            Path::new("./docs/plan-language-spec.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
    }

    #[test]
    fn should_revalidate_filters_irrelevant_events() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/states.yaml"),
        );

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(should_revalidate(&event, &watched));

        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(!should_revalidate(&event, &watched));
    }

    #[test]
    fn should_revalidate_matches_workspace_task_files() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let workspace = dir.path().join("workspace");
        let tasks = workspace.join("tasks");
        fs::create_dir_all(&tasks).expect("workspace tasks dir");
        fs::write(workspace.join("index.rhei.md"), "# Rhei: Watch\n").expect("index");
        let task_path = tasks.join("alpha.md");
        fs::write(&task_path, "### Task 1: Alpha\n**State:** pending\n").expect("task");

        let watch_plan = validation_watch_plan(&workspace, None);
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![task_path],
            attrs: Default::default(),
        };

        assert!(should_revalidate(&event, &watch_plan.targets));
    }

    #[test]
    fn validation_watch_plan_does_not_require_a_valid_initial_plan() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let broken_plan = dir.path().join("broken.rhei.md");

        let watch_plan = validation_watch_plan(&broken_plan, None);

        assert!(path_matches(&broken_plan, &watch_plan.targets));
        assert!(watch_plan
            .roots
            .iter()
            .any(|root| root.path == canonical_watch_path(dir.path())
                && root.mode == RecursiveMode::NonRecursive));
    }

    #[test]
    fn watch_targets_match_created_relative_missing_plan() {
        fs::create_dir_all("target").expect("target dir");
        let current_thread = std::thread::current();
        let thread_name = current_thread.name().unwrap_or("unnamed");
        let rel_path = PathBuf::from(format!(
            "target/rhei-watch-missing-{}-{}.rhei.md",
            std::process::id(),
            thread_name
        ));
        let _ = fs::remove_file(&rel_path);

        let watch_plan = validation_watch_plan(&rel_path, None);
        fs::write(&rel_path, "# Rhei: Created Later\n\n## Tasks\n").expect("create plan");
        let created_path = rel_path.canonicalize().expect("canonical created plan");

        assert!(path_matches(&created_path, &watch_plan.targets));

        fs::remove_file(&rel_path).expect("cleanup created plan");
    }

    #[test]
    fn parses_complete_command_with_result() {
        let cli = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "3",
            "--result",
            "All tests pass",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Complete { input, task, result, no_callbacks } => {
                assert_eq!(input, PathBuf::from("plan.rhei.md"));
                assert_eq!(task, "3");
                assert_eq!(result, "All tests pass");
                assert!(!no_callbacks);
            }
            other => panic!("expected complete command, got {other:?}"),
        }
    }

    #[test]
    fn parses_complete_command_requires_result() {
        // --result is mandatory; omitting it should fail.
        let err = Cli::try_parse_from([
            "rhei",
            "complete",
            "plan.rhei.md",
            "--task",
            "build",
            "--no-callbacks",
        ]);
        assert!(err.is_err(), "complete without --result should fail");
    }

    #[test]
    fn parses_reset_command() {
        let cli = Cli::try_parse_from(["rhei", "reset", "workspace"]).expect("cli should parse");

        match cli.command {
            Commands::Reset { input } => {
                assert_eq!(input, PathBuf::from("workspace"));
            }
            other => panic!("expected reset command, got {other:?}"),
        }
    }

    #[test]
    fn find_completion_state_prefers_non_cancelled_terminal() {
        let yaml = r#"
name: test
version: 1
states:
  active: { description: "working" }
  completed: { description: "done", final: true }
  cancelled: { description: "nope", final: true }
transitions:
  - from: active
    to: cancelled
  - from: active
    to: completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let target = find_completion_state("active", &machine);
        assert_eq!(target.as_deref(), Some("completed"));
    }

    #[test]
    fn find_completion_state_does_not_fall_back_to_cancelled() {
        let yaml = r#"
name: test
version: 1
states:
  active: { description: "working" }
  cancelled: { description: "nope", final: true }
transitions:
  - from: active
    to: cancelled
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let target = find_completion_state("active", &machine);
        assert!(target.is_none(), "complete should not treat cancellation as success");
    }

    #[test]
    fn find_completion_state_returns_none_when_no_terminal_reachable() {
        let yaml = r#"
name: test
version: 1
states:
  draft: { description: "initial", initial: true }
  pending: { description: "ready" }
  completed: { description: "done", final: true }
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        // draft can only go to pending (non-terminal), not directly to completed
        let target = find_completion_state("draft", &machine);
        assert!(target.is_none());
    }

    #[test]
    fn complete_command_blocks_gating_state() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        fs::write(
            &plan,
            "# Rhei: Gate\n\n## Tasks\n\n### Task 1: Review\n**State:** human-review\n\nReview.\n",
        )
        .expect("write plan");
        fs::write(
            &states,
            "name: test\nversion: 1\nstates:\n  human-review:\n    description: review\n    gating: true\n  completed:\n    description: done\n    final: true\ntransitions:\n  - from: human-review\n    to: completed\n",
        )
        .expect("write states");

        let err = complete_command(&plan, Some(&states), "1", "done", true)
            .expect_err("gating completion must fail");
        assert!(err.to_string().contains("gating state"), "{err}");
    }

    #[test]
    fn rewrite_task_completion_removes_assignee_and_appends_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"# Rhei: Test

## Tasks

### Task 1: Alpha
**State:** completed
**Assignee:** agent-1
Some work description.
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", "1", "runtime/results/1.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"), "assignee should be removed");
        assert!(
            content.contains("> **Result:** [1](runtime/results/1.md)"),
            "result link should be appended"
        );
        // State line should remain
        assert!(content.contains("**State:** completed"));
    }

    #[test]
    fn rewrite_task_completion_without_assignee_still_appends_result_link() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 1: Alpha
**State:** completed
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "1", "1", "runtime/results/1.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("> **Result:** [1](runtime/results/1.md)"));
    }

    #[test]
    fn rewrite_all_states_to_initial_updates_tasks_and_children() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Alpha
**State:** completed

#### Task 1.1: Detail
**State:** in-progress

### Task 2: Beta
**State:** review
"#;

        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: reset-test
version: 1
states:
  pending:
    description: Ready
    initial: true
  completed:
    description: Done
    final: true
"#,
        )
        .expect("load reset machine");
        let rewritten = rewrite_all_states_to_initial(raw, &machine).expect("rewrite states");

        assert_eq!(rewritten.matches("**State:** pending").count(), 3);
        assert!(!rewritten.contains("**State:** completed"));
        assert!(!rewritten.contains("**State:** in-progress"));
        assert!(!rewritten.contains("**State:** review"));
    }

    #[test]
    fn rewrite_all_states_to_initial_uses_resolved_profile_per_node() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Alpha
**State:** completed

#### Task 1.1: Detail
**State:** completed
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: profile-reset
version: 3
states:
  draft: { description: Draft }
  pending: { description: Pending }
  completed: { description: Done, final: true }
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
profiles:
  reviewed:
    initial: draft
    allowed: [draft, pending, completed]
  simple:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: reviewed
  default: reviewed
  overrides:
    - match: { level: 2 }
      profile: simple
"#,
        )
        .expect("load profile reset machine");

        let rewritten = rewrite_all_states_to_initial(raw, &machine).expect("rewrite states");

        assert!(rewritten.contains("### Task 1: Alpha\n**State:** draft"));
        assert!(rewritten.contains("#### Task 1.1: Detail\n**State:** pending"));
    }

    #[test]
    fn rewrite_task_completion_inserts_result_link_before_child() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Task 2: Beta
**State:** completed

Body text.

#### Task 2.1: Sub
**State:** completed
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "2", "2", "runtime/results/2.md", true).expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        let result_pos =
            content.find("> **Result:** [2](runtime/results/2.md)").expect("result present");
        let child_pos = content.find("#### Task 2.1").expect("child present");
        assert!(result_pos < child_pos, "result should appear before child");
    }

    #[test]
    fn rewrite_task_completion_matches_custom_node_kind() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("plan.md");
        fs::write(
            &path,
            r#"### Bug cache-key: Fix cache
**State:** completed
**Assignee:** codex

Body text.
"#,
        )
        .expect("write");

        rewrite_task_completion(&path, "cache-key", "cache-key", "runtime/results/cache-key.md", true)
            .expect("rewrite");

        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains("**Assignee:**"), "assignee should be removed");
        assert!(content.contains("> **Result:** [cache-key](runtime/results/cache-key.md)"));
    }

    #[test]
    fn reset_rewrite_strips_assignees() {
        let raw = r#"# Rhei: Reset

## Tasks

### Task 1: Alpha
**State:** completed
**Assignee:** codex

#### Task 1.1: Detail
**State:** completed
**Assignee:** claude-code
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: reset-assignee
version: 1
states:
  draft:
    description: Ready
    initial: true
  completed:
    description: Done
    final: true
"#,
        )
        .expect("load reset machine");
        let rewritten = rewrite_all_states_to_initial(raw, &machine).expect("rewrite states");
        let rewritten = strip_assignee_lines(&rewritten);

        assert_eq!(rewritten.matches("**State:** draft").count(), 2);
        assert!(!rewritten.contains("**Assignee:**"));
    }

    #[test]
    fn clap_command_factory_builds() {
        Cli::command().debug_assert();
    }

    // ---- MCP / skills resolution ----

    fn machine_with_tooling(state_yaml: &str) -> rhei_validator::StateMachine {
        let yaml = format!(
            "name: tooling-test\nversion: 1\nstates:\n{state_yaml}\n  completed:\n    description: done\n    final: true\n"
        );
        rhei_validator::StateMachine::from_yaml_str(&yaml).expect("valid state machine")
    }

    fn settings_with(
        defaults_mcp: Option<Vec<StateMcpEntry>>,
        registry: BTreeMap<String, McpServerProfile>,
    ) -> RheiSettings {
        RheiSettings {
            agent: None,
            agent_mode: None,
            model: None,
            agent_timeout: None,
            program_timeout: None,
            defaults: SettingsDefaults {
                model: None,
                agent: None,
                agent_mode: None,
                agent_timeout: None,
                program_timeout: None,
                mcp_servers: defaults_mcp,
                skills: None,
            },
            agents: built_in_agents(),
            models: BTreeMap::new(),
            mcp_servers: registry,
            skills: BTreeMap::new(),
            snapshots: None,
        }
    }

    #[test]
    fn resolve_tooling_unions_defaults_with_state_overrides_by_id() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: linear
        optional: true
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("linear".to_string(), McpServerProfile::default());
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings = settings_with(
            Some(vec![
                StateMcpEntry::Id("postgres".to_string()),
                StateMcpEntry::Id("linear".to_string()),
            ]),
            registry,
        );

        let tooling = resolve_tooling(&machine, "pending", &settings);
        // postgres from defaults stays first; linear from defaults is replaced
        // by the state-level entry that flips optional to true.
        let ids: Vec<&str> = tooling.mcp_servers.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["postgres", "linear"]);
        let linear = tooling.mcp_servers.iter().find(|e| e.id == "linear").unwrap();
        assert!(linear.optional, "state override should win");
        assert!(linear.definition.is_some(), "registry entry resolves");
    }

    #[test]
    fn resolve_tooling_empty_state_list_clears_defaults() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers: []
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings =
            settings_with(Some(vec![StateMcpEntry::Id("postgres".to_string())]), registry);

        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert!(tooling.mcp_servers.is_empty(), "explicit empty clears defaults");
    }

    #[test]
    fn resolve_tooling_omitted_state_inherits_defaults() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
"#,
        );
        let mut registry = BTreeMap::new();
        registry.insert("postgres".to_string(), McpServerProfile::default());
        let settings =
            settings_with(Some(vec![StateMcpEntry::Id("postgres".to_string())]), registry);

        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        assert_eq!(tooling.mcp_servers[0].id, "postgres");
    }

    #[test]
    fn resolve_tooling_inline_definition_does_not_require_registry() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers:
      - id: adhoc
        command: ["mcp-adhoc", "--port", "8080"]
"#,
        );
        let settings = settings_with(None, BTreeMap::new());
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        let entry = &tooling.mcp_servers[0];
        assert_eq!(entry.id, "adhoc");
        assert!(entry.definition.is_some(), "inline definition resolves");
        assert_eq!(entry.definition.as_ref().unwrap().command.as_deref().unwrap()[0], "mcp-adhoc");
    }

    #[test]
    fn resolve_tooling_unknown_id_resolves_to_unavailable() {
        let machine = machine_with_tooling(
            r#"  pending:
    description: Work
    agent: claude-code
    mcp_servers: [missing]
"#,
        );
        let settings = settings_with(None, BTreeMap::new());
        let tooling = resolve_tooling(&machine, "pending", &settings);
        assert_eq!(tooling.mcp_servers.len(), 1);
        assert!(
            tooling.mcp_servers[0].definition.is_none(),
            "unknown id has no definition (Half B reports it as unavailable)"
        );
        assert!(!tooling.mcp_available("missing"));
    }

    #[test]
    fn env_id_segment_normalizes_id() {
        assert_eq!(env_id_segment("linear"), "LINEAR");
        assert_eq!(env_id_segment("ad-hoc"), "AD_HOC");
        assert_eq!(env_id_segment("foo bar"), "FOO_BAR");
        assert_eq!(env_id_segment("a.b.c"), "A_B_C");
    }

    #[test]
    fn format_tooling_log_line_marks_unavailable_optional_with_question_mark() {
        let entries = vec![
            ResolvedMcpEntry {
                id: "postgres".to_string(),
                optional: false,
                definition: Some(McpServerProfile::default()),
            },
            ResolvedMcpEntry { id: "grafana".to_string(), optional: true, definition: None },
        ];
        let line = format_tooling_log_line(&entries, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        assert_eq!(line.as_deref(), Some("postgres,grafana?"));
    }
