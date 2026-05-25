
/// Short description for a skill, used in registration blocks.
fn skill_description(name: &str) -> &'static str {
    match name {
        "rhei-plan-writer" => "create and validate Rhei Plans",
        "rhei-plan-worker" => "execute tasks in a Rhei Plan",
        "rhei-state-machine-writer" => "design custom state machines from project specs and teams",
        "rhei-template-writer" => "design reusable Rhei templates",
        _ => "rhei skill",
    }
}

/// Install skills for Cursor (`.mdc` format).
fn install_cursor(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    _link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".cursor")
    } else {
        home_dir()?.join(".cursor")
    };

    let rules_dir = base.join("rules");

    for (name, source) in skill_sources {
        let skill_md = source.join("SKILL.md");
        let content = fs::read_to_string(&skill_md)
            .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;

        let description = skill_description(name);
        let mdc_content = format!(
            "---\ndescription: {description}\nglobs:\n  - \"**/*.rhei.md\"\nalwaysApply: false\n---\n\n{content}"
        );

        let dest = rules_dir.join(format!("{name}.mdc"));

        if dry_run {
            println!("  [dry-run] would write {}", dest.display());
            continue;
        }

        fs::create_dir_all(&rules_dir)
            .map_err(|e| miette!("failed to create '{}': {e}", rules_dir.display()))?;
        fs::write(&dest, &mdc_content)
            .map_err(|e| miette!("failed to write '{}': {e}", dest.display()))?;

        println!("  ✓ {} — written", dest.display());
    }

    Ok(())
}

/// Install skills for agents that use a simple rules directory (Kilocode, Pi, Antigravity).
fn install_rules_dir_agent(
    dir_name: &str,
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(dir_name)
    } else {
        home_dir()?.join(dir_name)
    };

    let rules_dir = base.join("rules");

    for (name, source) in skill_sources {
        let dest = rules_dir.join(format!("{name}.md"));

        if link {
            let skill_md = source.join("SKILL.md");
            let src = if local { relative_path(&rules_dir, &skill_md) } else { skill_md };
            link_skill(&src, &dest, dry_run)?;
        } else {
            let skill_md = source.join("SKILL.md");
            let content = fs::read_to_string(&skill_md)
                .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;

            if dry_run {
                println!("  [dry-run] would write {}", dest.display());
                continue;
            }

            fs::create_dir_all(&rules_dir)
                .map_err(|e| miette!("failed to create '{}': {e}", rules_dir.display()))?;
            fs::write(&dest, &content)
                .map_err(|e| miette!("failed to write '{}': {e}", dest.display()))?;

            println!("  ✓ {} — written", dest.display());
        }
    }

    Ok(())
}

/// Install skills for Windsurf (marker injection).
fn install_windsurf(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let file = if local {
        project_root
            .ok_or_else(|| miette!("--local requires a project root"))?
            .join(".windsurfrules")
    } else {
        // Check alternative global path first.
        let alt = home_dir()?.join(".codeium/windsurf/memories/global_rules.md");
        if alt.exists() {
            alt
        } else {
            home_dir()?.join(".windsurfrules")
        }
    };

    let content = build_marker_content(skill_sources)?;
    inject_marked_section(&file, &content, dry_run)?;

    if !dry_run {
        println!("  ✓ {} — appended rhei section", file.display());
    }

    Ok(())
}

/// Install skills for Copilot (marker injection).
fn install_copilot(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let file = if local {
        project_root
            .ok_or_else(|| miette!("--local requires a project root"))?
            .join(".github/copilot-instructions.md")
    } else {
        home_dir()?.join(".github/copilot-instructions.md")
    };

    let content = build_marker_content(skill_sources)?;
    inject_marked_section(&file, &content, dry_run)?;

    if !dry_run {
        println!("  ✓ {} — appended rhei section", file.display());
    }

    Ok(())
}

/// Install skills for Codex.
fn install_codex(
    skill_sources: &[(String, PathBuf)],
    local: bool,
    link: bool,
    dry_run: bool,
    project_root: Option<&Path>,
) -> MietteResult<()> {
    let base = if local {
        project_root.ok_or_else(|| miette!("--local requires a project root"))?.join(".agents")
    } else {
        home_dir()?.join(".agents")
    };

    let skills_dir = base.join("skills");

    // Install each skill directory. Codex discovers skills by scanning `.agents/skills`
    // (repo-local) and `$HOME/.agents/skills` (user-level).
    for (name, source) in skill_sources {
        if link {
            let dest = skills_dir.join(name);
            let src =
                if local { relative_path(dest.parent().unwrap(), source) } else { source.clone() };
            link_skill(&src, &dest, dry_run)?;
        } else {
            let dest = skills_dir.join(name);
            copy_skill(source, &dest, dry_run)?;
        }
    }

    Ok(())
}

/// Build the content for marker-injected agents (Windsurf, Copilot).
fn build_marker_content(skill_sources: &[(String, PathBuf)]) -> MietteResult<String> {
    let mut parts = Vec::new();
    for (name, source) in skill_sources {
        let skill_md = source.join("SKILL.md");
        let content = fs::read_to_string(&skill_md)
            .map_err(|e| miette!("failed to read '{}': {e}", skill_md.display()))?;
        parts.push(format!(
            "## rhei-{name}\n\nWhen the user asks to create/execute a Rhei plan, follow these instructions:\n\n{content}"
        ));
    }
    Ok(parts.join("\n\n"))
}

/// Uninstall skills for a single agent.
fn uninstall_agent(
    agent: &Agent,
    local: bool,
    dry_run: bool,
    skills: &[String],
    project_root: Option<&Path>,
) -> MietteResult<()> {
    match agent {
        Agent::ClaudeCode => {
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".claude")
            } else {
                home_dir()?.join(".claude")
            };

            // Remove skill directories.
            for skill in skills {
                let dest = base.join("skills").join(skill);
                remove_path(&dest, dry_run)?;
            }

            // Remove registration from CLAUDE.md.
            let claude_md = base.join("CLAUDE.md");
            remove_marked_section(&claude_md, dry_run)?;
        }
        Agent::Cursor => {
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".cursor")
            } else {
                home_dir()?.join(".cursor")
            };

            for skill in skills {
                let dest = base.join("rules").join(format!("{skill}.mdc"));
                remove_path(&dest, dry_run)?;
            }
        }
        Agent::Windsurf => {
            let file = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".windsurfrules")
            } else {
                let alt = home_dir()?.join(".codeium/windsurf/memories/global_rules.md");
                if alt.exists() {
                    alt
                } else {
                    home_dir()?.join(".windsurfrules")
                }
            };
            remove_marked_section(&file, dry_run)?;
        }
        Agent::Copilot => {
            let file = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".github/copilot-instructions.md")
            } else {
                home_dir()?.join(".github/copilot-instructions.md")
            };
            remove_marked_section(&file, dry_run)?;
        }
        Agent::Codex => {
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(".agents")
            } else {
                home_dir()?.join(".agents")
            };

            for skill in skills {
                let dest = base.join("skills").join(skill);
                remove_path(&dest, dry_run)?;
            }
        }
        Agent::Kilocode | Agent::Pi | Agent::Antigravity => {
            let dir_name = match agent {
                Agent::Kilocode => ".kilocode",
                Agent::Pi => ".pi",
                Agent::Antigravity => ".antigravity",
                _ => unreachable!(),
            };
            let base = if local {
                project_root
                    .ok_or_else(|| miette!("--local requires a project root"))?
                    .join(dir_name)
            } else {
                home_dir()?.join(dir_name)
            };

            for skill in skills {
                let dest = base.join("rules").join(format!("{skill}.md"));
                remove_path(&dest, dry_run)?;
            }
        }
        Agent::All => {} // handled by expand_agent_list
    }

    println!("  ✓ uninstalled");
    Ok(())
}

/// Remove a file or directory, printing what was done.
fn remove_path(path: &Path, dry_run: bool) -> MietteResult<()> {
    if !path.exists() && path.symlink_metadata().is_err() {
        return Ok(());
    }

    if dry_run {
        println!("  [dry-run] would remove {}", path.display());
        return Ok(());
    }

    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir_all(path)
            .map_err(|e| miette!("failed to remove '{}': {e}", path.display()))?;
    } else {
        fs::remove_file(path).map_err(|e| miette!("failed to remove '{}': {e}", path.display()))?;
    }

    Ok(())
}

/// Recursively copy a skill directory to a destination.
///
/// Creates parent directories as needed. Prints `✓ <dest> — written` on
/// success.
fn copy_skill(src: &Path, dest: &Path, dry_run: bool) -> MietteResult<()> {
    if dry_run {
        println!("  [dry-run] would copy {} → {}", src.display(), dest.display());
        return Ok(());
    }

    if dest.exists() {
        fs::remove_dir_all(dest)
            .map_err(|e| miette!("failed to remove existing '{}': {e}", dest.display()))?;
    }

    copy_dir_recursive(src, dest)?;

    println!("  ✓ {} — written", dest.display());
    Ok(())
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dest: &Path) -> MietteResult<()> {
    fs::create_dir_all(dest)
        .map_err(|e| miette!("failed to create directory '{}': {e}", dest.display()))?;

    for entry in fs::read_dir(src)
        .map_err(|e| miette!("failed to read directory '{}': {e}", src.display()))?
    {
        let entry = entry.map_err(|e| miette!("failed to read dir entry: {e}"))?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| {
                miette!("failed to copy '{}' → '{}': {e}", src_path.display(), dest_path.display())
            })?;
        }
    }

    Ok(())
}

/// Create a symlink from `dest` to `src`.
///
/// For local installs, callers should pass a relative `src` path so the
/// project stays portable. Prints `✓ <dest> → <src>` on success.
fn link_skill(src: &Path, dest: &Path, dry_run: bool) -> MietteResult<()> {
    if dry_run {
        println!("  [dry-run] would symlink {} → {}", dest.display(), src.display());
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create directory '{}': {e}", parent.display()))?;
    }

    // Remove existing symlink or directory.
    if dest.symlink_metadata().is_ok() {
        if dest.is_dir() && !dest.is_symlink() {
            fs::remove_dir_all(dest)
                .map_err(|e| miette!("failed to remove existing '{}': {e}", dest.display()))?;
        } else {
            fs::remove_file(dest)
                .map_err(|e| miette!("failed to remove existing '{}': {e}", dest.display()))?;
        }
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dest).map_err(|e| {
            miette!("failed to symlink '{}' → '{}': {e}", dest.display(), src.display())
        })?;
        println!("  ✓ {} → {}", dest.display(), src.display());
        Ok(())
    }

    #[cfg(not(unix))]
    {
        Err(miette!("symlinks are only supported on Unix platforms"))
    }
}
