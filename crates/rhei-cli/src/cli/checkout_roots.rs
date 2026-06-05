#[derive(Debug, Clone)]
struct AgentCheckoutRoot {
    path: PathBuf,
    worktree_root: Option<PathBuf>,
}

#[derive(Debug, serde::Deserialize)]
struct TaskWorktreeRef {
    path: PathBuf,
}

fn resolve_agent_checkout_root(
    rhei_root: &Path,
    task_id: &str,
) -> MietteResult<AgentCheckoutRoot> {
    // §FS-rhei-agents.4: agent cwd prefers task worktree, then repo root, then invocation cwd.
    if let Some(worktree_root) = read_task_worktree_root(rhei_root, task_id)? {
        return Ok(AgentCheckoutRoot { path: worktree_root.clone(), worktree_root: Some(worktree_root) });
    }

    if let Some(git_root) = git_toplevel(rhei_root) {
        return Ok(AgentCheckoutRoot { path: git_root, worktree_root: None });
    }

    let cwd = std::env::current_dir()
        .map_err(|err| miette!("failed to determine current working directory: {err}"))?;
    Ok(AgentCheckoutRoot { path: cwd, worktree_root: None })
}

fn task_worktree_ref_path(rhei_root: &Path, task_id: &str) -> PathBuf {
    rhei_root.join("runtime").join("worktree-refs").join(format!("{task_id}.yaml"))
}

fn read_task_worktree_root(rhei_root: &Path, task_id: &str) -> MietteResult<Option<PathBuf>> {
    let ref_path = task_worktree_ref_path(rhei_root, task_id);
    if !ref_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&ref_path).map_err(|err| {
        file_io_report(&ref_path, "failed to read task worktree reference", err)
    })?;
    let parsed: TaskWorktreeRef = serde_yaml::from_str(&raw)
        .map_err(|err| miette!("failed to parse task worktree reference '{}': {err}", ref_path.display()))?;

    // §FS-rhei-agents.4: worktree refs must point at absolute git worktree roots.
    if !parsed.path.is_absolute() {
        return Err(miette!(
            "task worktree reference '{}' must contain an absolute path",
            ref_path.display()
        ));
    }
    let worktree_root = parsed.path.canonicalize().map_err(|err| {
        file_io_report(&parsed.path, "failed to resolve task worktree root", err)
    })?;
    if !worktree_root.is_dir() {
        return Err(miette!(
            "task worktree reference '{}' points to a non-directory path '{}'",
            ref_path.display(),
            worktree_root.display()
        ));
    }

    let git_root = git_toplevel_required(&worktree_root)?;
    if git_root != worktree_root {
        return Err(miette!(
            "task worktree reference '{}' points to '{}', but that path's git root is '{}'",
            ref_path.display(),
            worktree_root.display(),
            git_root.display()
        ));
    }
    Ok(Some(worktree_root))
}

fn git_toplevel(path: &Path) -> Option<PathBuf> {
    git_toplevel_output(path).ok()
}

fn git_toplevel_required(path: &Path) -> MietteResult<PathBuf> {
    git_toplevel_output(path).map_err(|message| {
        miette!(
            "task worktree root '{}' is not a valid git worktree: {message}",
            path.display()
        )
    })
}

fn git_toplevel_output(path: &Path) -> Result<PathBuf, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git rev-parse exited with {}", output.status)
        } else {
            stderr
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let root = stdout.trim();
    if root.is_empty() {
        return Err("git rev-parse returned an empty toplevel".to_string());
    }
    PathBuf::from(root).canonicalize().map_err(|err| err.to_string())
}
