    use super::*;
    use clap::CommandFactory;

    #[test]
    fn parses_validate_command_with_input() {
        let cli = Cli::try_parse_from(["rhei", "validate", "docs/markdown-plan-compiler.md"])
            .expect("cli should parse");

        assert!(cli.state_machine.is_none());
        match cli.command {
            Commands::Validate { watch, input } => {
                assert!(!watch);
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_validate_watch_command_with_input() {
        let cli =
            Cli::try_parse_from(["rhei", "validate", "--watch", "docs/markdown-plan-compiler.md"])
                .expect("cli should parse");

        assert!(cli.state_machine.is_none());
        match cli.command {
            Commands::Validate { watch, input } => {
                assert!(watch);
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_json_pretty() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "json",
            "--pretty",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { input, format, pretty, no_color, no_metadata, no_content } => {
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
                assert_eq!(format, RenderFormat::Json);
                assert!(pretty);
                assert!(!no_color);
                assert!(!no_metadata);
                assert!(!no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_github_toggles() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "github",
            "--no-metadata",
            "--no-content",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { format, no_metadata, no_content, .. } => {
                assert_eq!(format, RenderFormat::Github);
                assert!(no_metadata);
                assert!(no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_progress_no_color() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "progress",
            "--no-color",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render { format, no_color, .. } => {
                assert_eq!(format, RenderFormat::Progress);
                assert!(no_color);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_states_command() {
        let cli = Cli::try_parse_from(["rhei", "states"]).expect("cli should parse");
        match cli.command {
            Commands::States { json } => assert!(!json),
            other => panic!("expected states command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["rhei", "states", "--json"]).expect("cli should parse");
        match cli.command {
            Commands::States { json } => assert!(json),
            other => panic!("expected states command, got {other:?}"),
        }
    }

    #[test]
    fn render_state_machine_text_includes_states_and_transitions() {
        let yaml = r#"
name: demo
version: 1
models:
  - gpt-5
  - claude-sonnet
states:
  draft:
    description: planning
    instructions: Wait until author promotes task.
    personality: Ask one sharp planning question first.
    initial: true
    visits: 3
    all_models:
      - gpt-5
      - claude-sonnet
  done:
    description: finished
    model: gpt-5
    final: true
transitions:
  - from: draft
    to: done
    on_enter: cli:record_done
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_text(&machine);

        assert!(rendered.contains("State machine: demo"));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("draft [initial]"));
        assert!(rendered.contains("Visits: 3"));
        assert!(rendered.contains("Models: gpt-5, claude-sonnet"));
        assert!(rendered.contains("Personality: Ask one sharp planning question first."));
        assert!(rendered.contains("Wait until author promotes task."));
        assert!(rendered.contains("done [final]"));
        assert!(rendered.contains("Model: gpt-5"));
        assert!(rendered.contains("draft -> done (on_enter=cli:record_done)"));
    }

    #[test]
    fn render_state_machine_json_includes_state_personality() {
        let yaml = r#"
name: demo
version: 1
models:
  - gpt-5
states:
  draft:
    description: planning
    personality: Focus on planning risks.
    visits: 2
    all_models:
      - gpt-5
    initial: true
  done:
    description: done
    final: true
transitions: []
"#;
        let machine = rhei_validator::StateMachine::from_yaml_str(yaml).expect("load");
        let rendered = render_state_machine_json(&machine).expect("render JSON");
        let json: serde_json::Value = serde_json::from_str(&rendered).expect("parse JSON");

        assert_eq!(json["name"], "demo");
        assert_eq!(json["models"], serde_json::json!(["gpt-5"]));
        assert_eq!(json["states"][0]["personality"], "Focus on planning risks.");
        assert_eq!(json["states"][0]["visits"], 2);
        assert_eq!(json["states"][0]["all_models"], serde_json::json!(["gpt-5"]));
    }

    #[test]
    fn parses_run_command_with_separated_flag_groups() {
        let cli = Cli::try_parse_from([
            "rhei",
            "run",
            "plan.rhei.md",
            "--dry-run",
            "--no-callbacks",
            "--continue-on-error",
            "--parallel",
            "4",
            "--no-agent",
            "--agent",
            "codex",
            "--model",
            "o3",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Run { input, standalone, agent, program, snapshot } => {
                assert_eq!(input, PathBuf::from("plan.rhei.md"));
                assert!(standalone.dry_run);
                assert!(standalone.no_callbacks);
                assert!(standalone.continue_on_error);
                assert_eq!(standalone.parallel, 4);
                assert!(agent.no_agent);
                assert_eq!(agent.agent.as_deref(), Some("codex"));
                assert_eq!(agent.model.as_deref(), Some("o3"));
                assert!(!program.no_program);
                assert_eq!(program.program_timeout.as_deref(), None);
                assert!(snapshot.from_snapshot.is_none());
                assert!(!snapshot.override_inherit);
                assert!(snapshot.snapshot_task.is_none());
                assert!(snapshot.snapshot_target.is_none());
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_run_command_with_snapshot_flags() {
        let cli = Cli::try_parse_from([
            "rhei",
            "run",
            "plan.rhei.md",
            "--from-snapshot",
            "1.2.3:implementation:pending@2:claude-code-anthropic-claude-opus-4-7/g3",
            "--override-inherit",
            "--task",
            "1.2.3",
            "--target",
            "claude-code-anthropic-claude-opus-4-7",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Run { snapshot, .. } => {
                assert_eq!(
                    snapshot.from_snapshot.as_deref(),
                    Some("1.2.3:implementation:pending@2:claude-code-anthropic-claude-opus-4-7/g3")
                );
                assert!(snapshot.override_inherit);
                assert_eq!(snapshot.snapshot_task.as_deref(), Some("1.2.3"));
                assert_eq!(
                    snapshot.snapshot_target.as_deref(),
                    Some("claude-code-anthropic-claude-opus-4-7")
                );
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_rejects_override_inherit_without_from_snapshot() {
        let err = Cli::try_parse_from(["rhei", "run", "plan.rhei.md", "--override-inherit"])
            .expect_err("clap should reject --override-inherit without --from-snapshot");
        let msg = err.to_string();
        assert!(
            msg.contains("--from-snapshot") || msg.contains("requires"),
            "unexpected clap error: {msg}"
        );
    }

    #[test]
    fn run_help_separates_standalone_and_agent_flags() {
        let mut command = Cli::command();
        let run = command.find_subcommand_mut("run").expect("run subcommand should exist");
        let mut buffer = Vec::new();
        run.write_long_help(&mut buffer).expect("help should render");
        let help = String::from_utf8(buffer).expect("help should be UTF-8");

        assert!(help.contains("Standalone Execution:"));
        assert!(help.contains("--dry-run"));
        assert!(help.contains("--parallel"));
        assert!(help.contains("Agent Execution:"));
        assert!(help.contains("--no-agent"));
        assert!(help.contains("--agent <AGENT>"));
        assert!(help.contains("--model <MODEL>"));
        assert!(help.contains("Program Execution:"));
        assert!(help.contains("--no-program"));
        assert!(help.contains("--program-timeout <DURATION>"));
        // Snapshots flag group, per docs/functional-spec/rhei-run.spec.md
        // § Snapshots.
        assert!(help.contains("Snapshots:"));
        assert!(help.contains("--from-snapshot <REF>"));
        assert!(help.contains("--override-inherit"));
        assert!(help.contains("--task <TASK_ID>"));
        assert!(help.contains("--target <SLUG>"));
    }

    #[test]
    fn parses_version_command() {
        let cli = Cli::try_parse_from(["rhei", "version"]).expect("cli should parse");

        match cli.command {
            Commands::Version => {}
            other => panic!("expected version command, got {other:?}"),
        }
    }

    #[test]
    fn parses_completions_command() {
        let cli = Cli::try_parse_from(["rhei", "completions", "fish"]).expect("cli should parse");

        match cli.command {
            Commands::Completions { shell, install, system, output, dry_run, .. } => {
                assert_eq!(shell, CompletionShell::Fish);
                assert!(!install);
                assert!(!system);
                assert!(output.is_none());
                assert!(!dry_run);
            }
            other => panic!("expected completions command, got {other:?}"),
        }

        let cli =
            Cli::try_parse_from(["rhei", "completions", "powershell"]).expect("cli should parse");
        match cli.command {
            Commands::Completions { shell, .. } => assert_eq!(shell, CompletionShell::PowerShell),
            other => panic!("expected completions command, got {other:?}"),
        }
    }

    #[test]
    fn parses_completions_install_options() {
        let cli = Cli::try_parse_from([
            "rhei",
            "completions",
            "bash",
            "--install",
            "--system",
            "--dry-run",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Completions { shell, install, system, dry_run, .. } => {
                assert_eq!(shell, CompletionShell::Bash);
                assert!(install);
                assert!(system);
                assert!(dry_run);
            }
            other => panic!("expected completions command, got {other:?}"),
        }
    }

    #[test]
    fn root_help_lists_completions_command() {
        let mut command = Cli::command();
        let mut buffer = Vec::new();
        command.write_long_help(&mut buffer).expect("help should render");
        let help = String::from_utf8(buffer).expect("help should be UTF-8");

        assert!(help.contains("Setup:"));
        assert!(help.contains("completions"));
        assert!(help.contains("Generate shell completion scripts"));
    }

    #[test]
    fn render_rhei_json_smoke() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Smoke

## Tasks

### Task 1: Alpha
**State:** pending
"#,
        )
        .expect("parse should succeed");

        let rendered =
            render_rhei(&rhei, RenderFormat::Json, true, false, false, false).expect("render ok");

        assert!(rendered.contains("\"title\": \"Smoke\""));
        assert!(rendered.contains("\"tasks\""));
    }

    #[test]
    fn compose_agent_prompt_carries_domain_instructions_only() {
        let rhei = rhei_core::parse(
            r#"# Rhei: Prompt Smoke

## Tasks

### Task demo: Verify prompt wiring
**State:** review

Write findings and transition the task.
"#,
        )
        .expect("plan should parse");
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: prompt-smoke
version: 1
states:
  review:
    description: review
    instructions: Write findings to `{output.review-notes.path}`.
    initial: true
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}.md
  fix:
    description: fix
    final: true
transitions:
  - from: review
    to: fix
"#,
        )
        .expect("machine should parse");
        let task = &rhei.tasks[0];
        let context = RuntimeTemplateContext {
            workspace_root: Path::new("/tmp/workspace"),
            plan_path: Path::new("/tmp/workspace"),
            state_machine_path: Some(Path::new("/tmp/workspace/states.yaml")),
            plan_title: &rhei.title,
            task,
            state_name: "review",
            current_state_raw: "review",
            machine: &machine,
            metadata: None,
            target: None,
            model: None,
            agent: Some("codex"),
            agent_mode: None,
            tooling: None,
        };

        let prompt = compose_agent_prompt(&context);

        // New Rhei Commands section replaces Workflow Notes.
        assert!(prompt.contains("## Rhei Commands"));
        assert!(prompt.contains("rhei-managed plan at `/tmp/workspace`"));
        assert!(prompt.contains("The active state machine is `/tmp/workspace/states.yaml`."));
        assert!(prompt.contains(
            "The `rhei run` process that spawned you is responsible for advancing the task"
        ));
        assert!(prompt.contains("Available transitions from `review`:"));

        // Completion is a property of the execution model, not the prompt — no
        // completion prose should appear.
        assert!(!prompt.contains("then stop"));
        assert!(!prompt.contains("create every required output artifact"));
        assert!(!prompt.contains("produce every required output artifact"));
        assert!(!prompt.contains("for caller context"));
        assert!(!prompt.contains("Workflow Notes"));
    }

    #[test]
    fn parse_diagnostic_includes_line_info_when_available() {
        let input = "first line\nbad line\nthird line";
        let err = rhei_core::parser::ParseError {
            message: "unexpected token".to_string(),
            line: Some(2),
        };

        let rendered = render_parse_diagnostic(Path::new("broken.md"), input, &err);

        assert!(rendered.contains("-- PARSE ERROR"));
        assert!(rendered.contains("broken.md"));
        assert!(rendered.contains("2| bad line"));
        assert!(rendered.contains("unexpected token"));
    }

    #[test]
    fn validation_failure_formatting_aggregates_multiple_errors() {
        let rendered = format_validation_errors(&[
            "Task 1 is missing mandatory **State:** metadata".to_string(),
            "Task 2 depends on missing Task 9".to_string(),
        ]);

        assert!(rendered.contains("I found 2 problems:"));
        assert!(rendered.contains("1. Task 1 is missing mandatory **State:** metadata"));
        assert!(rendered.contains("2. Task 2 depends on missing Task 9"));
    }

    #[test]
    fn path_matches_normalizes_paths() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/states.yaml"),
        );

        assert!(path_matches(Path::new("./docs/markdown-plan-compiler.md"), &watched));
