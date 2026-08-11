    /// Where an instantiated template lands relative to a Panta project. A
    /// member rhei keeps whatever machine it declares — the project default
    /// only covers rheis that declare nothing. §FS-rhei-templates.6.2
    pub(super) enum ProjectPlacement {
        /// The output is not a member of any project; nothing to reconcile.
        Standalone,
        /// The output joins `project` as a member rhei.
        Member { project: PathBuf },
    }

    impl ProjectPlacement {
        pub(super) fn project(&self) -> Option<&Path> {
            match self {
                ProjectPlacement::Standalone => None,
                ProjectPlacement::Member { project } => Some(project.as_path()),
            }
        }
    }

    /// The project a *new* rhei created from the current directory belongs to,
    /// following the same walk `rhei` uses to resolve an omitted target:
    /// the directory itself, then its `panta/` child. §FS-rhei-panta.6
    pub(super) fn enclosing_project_for_new_rhei(start: &Path) -> Option<PathBuf> {
        let mut current = Some(start);
        while let Some(dir) = current {
            if workspace::is_panta_project(dir) {
                return Some(dir.to_path_buf());
            }
            let nested = dir.join("panta");
            if workspace::is_panta_project(&nested) {
                return Some(nested);
            }
            current = dir.parent();
        }
        None
    }

    /// The project that would *discover* `output_dir`, which is only ever its
    /// immediate parent: discovery reads the entries sitting directly next to
    /// `index.panta.md` and never recurses. §AR-rhei-panta.1
    fn owning_project_of(output_dir: &Path) -> Option<PathBuf> {
        let parent = output_dir.parent()?;
        workspace::is_panta_project(parent).then(|| parent.to_path_buf())
    }

    /// Decide how `output_dir` relates to any enclosing project. State
    /// machines never conflict — a member rhei's own declaration simply
    /// overrides the project default — so the only placement questions left
    /// are about discovery.
    // §FS-rhei-templates.6.2
    pub(super) fn plan_project_placement(
        output_dir: &Path,
        materialized_dir: &Path,
    ) -> MietteResult<ProjectPlacement> {
        let Some(project) = owning_project_of(output_dir) else {
            // Not a member — but a workspace buried deeper inside a project is
            // invisible to it, and silently producing something the project
            // will never list is the failure this whole check exists to stop.
            if let Some(outer) = enclosing_project_for_new_rhei(output_dir) {
                if output_dir.starts_with(&outer) {
                    eprintln!(
                        "warning: {} is inside the Panta project at {}, but not directly next to \
                         index.panta.md, so the project will not discover it. Instantiate into \
                         {} to make it a member.",
                        display_path(output_dir).display(),
                        display_path(&outer).display(),
                        display_path(&outer.join(output_dir.file_name().unwrap_or_default()))
                            .display()
                    );
                }
            }
            return Ok(ProjectPlacement::Standalone);
        };

        // Discovery counts `*.rhei.md` files and Directory Workspaces, nothing
        // else. A single-file template renders into a plain directory holding
        // `plan.rhei.md`, which is neither. §AR-rhei-panta.1
        if !workspace::is_workspace(materialized_dir) {
            eprintln!(
                "warning: {} sits in the Panta project at {}, but a single-file template renders                  a plain directory, which discovery does not count as a rhei — only `*.rhei.md`                  files and Directory Workspaces. The project will not list it. Instantiate it                  outside the project (`--output {}`), or move the rendered plan file next to                  index.panta.md.",
                display_path(output_dir).display(),
                display_path(&project).display(),
                display_path(&sibling_output_suggestion(&project, output_dir)).display()
            );
            return Ok(ProjectPlacement::Standalone);
        }

        Ok(ProjectPlacement::Member { project })
    }

    /// A path outside the project, next to it, keeping the chosen directory name.
    fn sibling_output_suggestion(project: &Path, output_dir: &Path) -> PathBuf {
        let name = output_dir.file_name().unwrap_or_default();
        match project.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
            _ => PathBuf::from(name),
        }
    }

    const WORKSPACE_SETTINGS_RELATIVE_PATH: &str = ".agents/rhei/settings.json";

    /// What happened to a template's shipped agent settings when its workspace
    /// joined a project.
    pub(super) struct HoistedSettings {
        pub(super) added: Vec<String>,
        pub(super) kept: Vec<String>,
    }

    /// Move a member workspace's `.agents/rhei/settings.json` up to the project
    /// and merge it into what is already there, keeping existing project values
    /// so a template never redefines a configured agent. §FS-rhei-agents.1.1
    pub(super) fn hoist_workspace_settings_into_project(
        workspace: &Path,
        project: &Path,
    ) -> MietteResult<Option<HoistedSettings>> {
        let source = workspace.join(WORKSPACE_SETTINGS_RELATIVE_PATH);
        if !source.is_file() {
            return Ok(None);
        }
        let incoming = read_settings_value(&source)?;
        let target = project.join(WORKSPACE_SETTINGS_RELATIVE_PATH);
        let mut merged =
            if target.is_file() { read_settings_value(&target)? } else { serde_json::json!({}) };

        let mut added = Vec::new();
        let mut kept = Vec::new();
        merge_settings_value(&mut merged, &incoming, "", &mut added, &mut kept);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                file_io_report(parent, "failed to create the project settings directory", err)
            })?;
        }
        let rendered = serde_json::to_string_pretty(&merged)
            .map_err(|err| miette!("failed to render merged settings: {err}"))?;
        fs::write(&target, format!("{rendered}\n"))
            .map_err(|err| file_io_report(&target, "failed to write project settings", err))?;

        // Leaving the workspace copy in place would keep advertising settings
        // nothing reads.
        fs::remove_file(&source)
            .map_err(|err| file_io_report(&source, "failed to remove workspace settings", err))?;
        prune_empty_parents(&source, workspace);

        Ok(Some(HoistedSettings { added, kept }))
    }

    fn read_settings_value(path: &Path) -> MietteResult<serde_json::Value> {
        let content = fs::read_to_string(path)
            .map_err(|err| file_io_report(path, "failed to read settings", err))?;
        serde_json::from_str(&content)
            .map_err(|err| miette!("failed to parse {}: {err}", path.display()))
    }

    /// Deep-merge `incoming` into `target`, recording which leaf keys were added
    /// and which the target already defined differently.
    fn merge_settings_value(
        target: &mut serde_json::Value,
        incoming: &serde_json::Value,
        path: &str,
        added: &mut Vec<String>,
        kept: &mut Vec<String>,
    ) {
        let (Some(target_map), Some(incoming_map)) = (target.as_object_mut(), incoming.as_object())
        else {
            return;
        };
        for (key, value) in incoming_map {
            let child_path =
                if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
            match target_map.get_mut(key) {
                None => {
                    target_map.insert(key.clone(), value.clone());
                    added.push(child_path);
                }
                Some(existing) if existing.is_object() && value.is_object() => {
                    merge_settings_value(existing, value, &child_path, added, kept);
                }
                Some(existing) if existing == value => {}
                Some(_) => kept.push(child_path),
            }
        }
    }

    /// Remove directories left empty by the hoist, stopping at `stop_at`.
    fn prune_empty_parents(removed: &Path, stop_at: &Path) {
        let mut current = removed.parent();
        while let Some(dir) = current {
            if dir == stop_at || !dir.starts_with(stop_at) {
                return;
            }
            if fs::read_dir(dir).map(|mut entries| entries.next().is_some()).unwrap_or(true) {
                return;
            }
            if fs::remove_dir(dir).is_err() {
                return;
            }
            current = dir.parent();
        }
    }
