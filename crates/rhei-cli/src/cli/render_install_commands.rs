struct NextOutput<'a> {
    as_json: bool,
    peek: bool,
    task: &'a rhei_core::ast::Task,
    from_state: &'a str,
    to_state: &'a str,
    personality: Option<&'a str>,
    instructions: &'a str,
    agent_id: Option<&'a str>,
    model_id: Option<&'a str>,
}

/// Print the `next` command output in either human-readable or JSON format.
fn print_next_output(output: NextOutput<'_>) {
    fn child_json(task: &rhei_core::ast::Task) -> serde_json::Value {
        let children: Vec<serde_json::Value> = task.children.iter().map(child_json).collect();
        serde_json::json!({
            "id": task.id.to_string(),
            "kind": task.kind,
            "title": task.title,
            "state": task.state,
            "content": task.content.trim(),
            "children": children,
        })
    }

    if output.as_json {
        let children: Vec<serde_json::Value> =
            output.task.children.iter().map(child_json).collect();

        let mut obj = serde_json::json!({
            "task_id": output.task.id.to_string(),
            "kind": output.task.kind,
            "title": output.task.title,
            "from_state": output.from_state,
            "state": output.to_state,
            "personality": output.personality,
            "instructions": output.instructions,
            "content": output.task.content.trim(),
            "children": children,
        });
        if let Some(agent) = output.agent_id {
            obj["agent"] = serde_json::json!(agent);
        }
        if let Some(model) = output.model_id {
            obj["model"] = serde_json::json!(model);
        }
        println!("{}", serde_json::to_string_pretty(&obj).expect("JSON serialization"));
    } else {
        let transitioned = output.from_state != output.to_state;
        if output.peek {
            println!(
                "Task {} — current state: '{}' (read-only peek; not advanced)",
                output.task.id, output.to_state
            );
        } else if transitioned {
            println!(
                "Task {} claimed: '{}' -> '{}'",
                output.task.id, output.from_state, output.to_state
            );
        } else {
            println!("Task {} (already in '{}')", output.task.id, output.to_state);
        }
        if output.agent_id.is_some() || output.model_id.is_some() {
            let agent_str = output.agent_id.unwrap_or("none");
            let model_str = output.model_id.unwrap_or("default");
            println!("Agent: {}  |  Model: {}", agent_str, model_str);
        }
        if let Some(personality) = output.personality {
            println!();
            println!("Personality: {}", personality);
        }
        println!();
        println!("## Task {}: {}", output.task.id, output.task.title);
        if !output.task.content.trim().is_empty() {
            println!();
            println!("{}", output.task.content.trim());
        }
        if !output.task.children.is_empty() {
            println!();
            for child in &output.task.children {
                println!(
                    "  - {} {}: {} [{}]",
                    title_case_kind(&child.kind),
                    child.id,
                    child.title,
                    child.state
                );
                if !child.content.trim().is_empty() {
                    for line in child.content.trim().lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
        if !output.instructions.is_empty() {
            println!();
            println!("--- Instructions ({}) ---", output.to_state);
            println!("{}", output.instructions);
        }
    }
}

/// Execute the `render` subcommand for the selected output format.
fn render_command(
    input: &Path,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> MietteResult<()> {
    let rhei = parse_input_file(input)?;
    let rendered = render_rhei(&rhei, format, pretty, no_color, no_metadata, no_content)
        .map_err(|err| miette!("{err}"))?;
    println!("{rendered}");
    Ok(())
}

/// Render a parsed rhei into the requested output representation.
fn render_rhei(
    rhei: &rhei_core::ast::Rhei,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> Result<String> {
    match format {
        RenderFormat::Json => {
            if pretty {
                Ok(rhei_output::to_json_string_pretty(rhei))
            } else {
                let value = rhei_output::to_json_value(rhei);
                serde_json::to_string(&value).context("failed to serialize JSON output")
            }
        }
        RenderFormat::Github => Ok(rhei_output::GithubIssuesOutput {
            include_content: !no_content,
            include_metadata: !no_metadata,
        }
        .to_markdown(rhei)),
        RenderFormat::Progress => {
            let color = should_use_color(no_color);
            Ok(rhei_output::ProgressReportOutput { color, show_dependencies: true }.to_string(rhei))
        }
    }
}

/// Decide whether ANSI color should be emitted for progress output.
///
/// Precedence: explicit `--no-color` always wins. Otherwise honour the
/// `NO_COLOR` environment variable (any non-empty value disables color) and
/// fall back to stdout TTY detection.
fn should_use_color(no_color_flag: bool) -> bool {
    use std::io::IsTerminal;
    if no_color_flag {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Print versions for the CLI and the crates surfaced by this command.
fn print_versions() {
    println!("rhei-cli {}", env!("CARGO_PKG_VERSION"));
    println!("rhei-core {}", rhei_core::version());
    println!("rhei-validator {}", rhei_validator::version());
    println!("rhei-output {}", rhei_output::version());
}

/// Handler for the `install-skills` subcommand.
///
/// Resolves the agent list (expanding `All`), iterates over each agent,
/// and calls the appropriate install/uninstall handler.
fn install_skills_command(
    agent: Agent,
    local: bool,
    link: bool,
    uninstall: bool,
    dry_run: bool,
    skills: &[String],
) -> MietteResult<()> {
    let agents = expand_agent_list(agent);
    let mut installed_count = 0u32;

    let project_root = if local { Some(find_project_root()?) } else { None };

    // Resolve all skill sources up front.
    let mut skill_sources: Vec<(String, PathBuf)> = Vec::new();
    if !uninstall {
        for skill in skills {
            let source = resolve_skill_source(skill)?;
            skill_sources.push((skill.clone(), source));
        }
    }

    for ag in &agents {
        let label = agent_label(ag);
        let mode_suffix = if local { " (local)" } else { "" };
        println!("\n{}{}:", label, mode_suffix);

        let result = if uninstall {
            uninstall_agent(ag, local, dry_run, skills, project_root.as_deref())
        } else {
            install_agent(ag, local, link, dry_run, &skill_sources, project_root.as_deref())
        };

        match result {
            Ok(()) => installed_count += 1,
            Err(e) => eprintln!("  error: {e}"),
        }
    }

    let action = if uninstall { "Uninstalled" } else { "Installed" };
    let scope = if local { " locally" } else { "" };
    println!(
        "\n{} rhei skills{} for {} agent{}.",
        action,
        scope,
        installed_count,
        if installed_count == 1 { "" } else { "s" }
    );

    Ok(())
}

/// Expand the `All` agent variant into the full list of concrete agents.
fn expand_agent_list(agent: Agent) -> Vec<Agent> {
    if agent == Agent::All {
        vec![
            Agent::ClaudeCode,
            Agent::Cursor,
            Agent::Windsurf,
            Agent::Copilot,
            Agent::Kilocode,
            Agent::Pi,
            Agent::Codex,
            Agent::Antigravity,
        ]
    } else {
        vec![agent]
    }
}

/// Human-readable label for an agent.
fn agent_label(agent: &Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => "claude-code",
        Agent::Cursor => "cursor",
        Agent::Windsurf => "windsurf",
        Agent::Copilot => "copilot",
        Agent::Kilocode => "kilocode",
        Agent::Pi => "pi",
        Agent::Codex => "codex",
        Agent::Antigravity => "antigravity",
        Agent::All => "all",
    }
}

/// Home directory helper.
fn home_dir() -> MietteResult<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| miette!("HOME environment variable not set"))
}

/// Install skills for a single agent.
fn install_agent(
    agent: &Agent,
    local: bool,
    link: bool,
    dry_run: bool,
    skill_sources: &[(String, PathBuf)],
    project_root: Option<&Path>,
) -> MietteResult<()> {
    match agent {
        Agent::ClaudeCode => install_claude_code(skill_sources, local, link, dry_run, project_root),
        Agent::Cursor => install_cursor(skill_sources, local, link, dry_run, project_root),
        Agent::Windsurf => install_windsurf(skill_sources, local, dry_run, project_root),
        Agent::Copilot => install_copilot(skill_sources, local, dry_run, project_root),
        Agent::Kilocode => {
            install_rules_dir_agent(".kilocode", skill_sources, local, link, dry_run, project_root)
        }
        Agent::Pi => {
            install_rules_dir_agent(".pi", skill_sources, local, link, dry_run, project_root)
        }
        Agent::Codex => install_codex(skill_sources, local, link, dry_run, project_root),
        Agent::Antigravity => install_rules_dir_agent(
            ".antigravity",
            skill_sources,
            local,
            link,
            dry_run,
            project_root,
        ),
        Agent::All => Ok(()), // handled by expand_agent_list
    }
}

/// Install skills for Claude Code.
fn install_claude_code(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".claude")
    } else {
        home_dir()?.join(".claude")
    };

    let skills_dir = base.join("skills");

    // Install each skill directory.
    for (name, source) in skill_sources {
        let dest = skills_dir.join(name);
        if link {
            let src =
                if local { relative_path(dest.parent().unwrap(), source) } else { source.clone() };
            link_skill(&src, &dest, dry_run)?;
        } else {
            copy_skill(source, &dest, dry_run)?;
        }
    }

    // Generate and inject registration block into CLAUDE.md.
    let claude_md = base.join("CLAUDE.md");
    let mut block = String::from("# rhei\n");
    for (name, _) in skill_sources {
        let skill_path = if local {
            format!(".claude/skills/{name}/SKILL.md")
        } else {
            format!("~/.claude/skills/{name}/SKILL.md")
        };
        let description = skill_description(name);
        let trigger = format!("/{name}");
        block.push_str(&format!(
            "- **{name}** (`{skill_path}`) — {description}. Trigger: `{trigger}`\n"
        ));
    }
    let trigger_list: Vec<String> =
        skill_sources.iter().map(|(name, _)| format!("`/{name}`")).collect();
    block.push_str(&format!(
        "When the user types {}, invoke the Skill tool with the corresponding skill name before doing anything else.\n",
        trigger_list.join(", ")
    ));

    // Use heading-based injection for Claude Code (not HTML markers).
    inject_claude_md_section(&claude_md, &block, dry_run)?;

    println!("  ✓ {} — registered {} skills", claude_md.display(), skill_sources.len());

    Ok(())
}

/// Inject or replace a `# rhei` section in a CLAUDE.md file.
fn inject_claude_md_section(file: &Path, content: &str, dry_run: bool) -> MietteResult<()> {
    let existing = if file.exists() {
        fs::read_to_string(file).map_err(|e| miette!("failed to read '{}': {e}", file.display()))?
    } else {
        String::new()
    };

    // Check for existing `# rhei` section and replace it.
    let lines: Vec<&str> = existing.lines().collect();
    let mut new_lines: Vec<String> = Vec::new();
    let mut in_rhei_block = false;
    let mut replaced = false;

    for line in &lines {
        if !in_rhei_block {
            if *line == "# rhei" || *line == "## rhei" {
                in_rhei_block = true;
                // Insert new content here.
                for cl in content.lines() {
                    new_lines.push(cl.to_string());
                }
                replaced = true;
                continue;
            }
            new_lines.push(line.to_string());
        } else {
            // Check if we've hit a new heading of equal or higher level.
            let level = line.chars().take_while(|&c| c == '#').count();
            if level > 0 && level <= 2 && !line.starts_with("###") {
                in_rhei_block = false;
                new_lines.push(line.to_string());
            }
            // Skip lines in the old rhei block.
        }
    }

    if !replaced {
        // Append the section.
        if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true) {
            new_lines.push(String::new());
        }
        for cl in content.lines() {
            new_lines.push(cl.to_string());
        }
    }

    let mut final_content = new_lines.join("\n");
    if !final_content.ends_with('\n') {
        final_content.push('\n');
    }

    if dry_run {
        println!("  [dry-run] would update {}", file.display());
        return Ok(());
    }

    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create directory '{}': {e}", parent.display()))?;
    }
    fs::write(file, &final_content)
        .map_err(|e| miette!("failed to write '{}': {e}", file.display()))?;

    Ok(())
}
