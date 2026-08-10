    /// Where an instantiated template lands relative to a Panta project, and
    /// what the project must adopt before it can load the result. The project,
    /// not the workspace, owns the machine and settings. §FS-rhei-templates.6.2
    pub(super) enum ProjectPlacement {
        /// The output is not a member of any project; nothing to reconcile.
        Standalone,
        /// The output joins `project`, which already governs `machine`.
        Member { project: PathBuf },
        /// The output joins `project`, which has no machine of its own yet and
        /// can take the template's as the project default.
        Adopts { project: PathBuf, machine: String },
    }

    impl ProjectPlacement {
        pub(super) fn project(&self) -> Option<&Path> {
            match self {
                ProjectPlacement::Standalone => None,
                ProjectPlacement::Member { project } | ProjectPlacement::Adopts { project, .. } => {
                    Some(project.as_path())
                }
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

    /// Decide how `output_dir` relates to any enclosing project, refusing the
    /// combinations that would leave the project unloadable.
    ///
    /// The check runs before validation so a machine collision is reported as a
    /// collision, with both names and a way out, rather than surfacing later as
    /// a load error from every project-scoped command.
    pub(super) fn plan_project_placement(
        output_dir: &Path,
        materialized_dir: &Path,
        workspace_machine: Option<&str>,
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
                        output_dir.display(),
                        outer.display(),
                        outer.join(output_dir.file_name().unwrap_or_default()).display()
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
                output_dir.display(),
                project.display(),
                sibling_output_suggestion(&project, output_dir).display()
            );
            return Ok(ProjectPlacement::Standalone);
        }

        let declared = project_declared_machine(&project)?;
        // A template that declares nothing simply inherits whatever the project
        // governs, which is exactly the single-machine rule working as intended.
        let Some(machine) = workspace_machine else {
            return Ok(ProjectPlacement::Member { project });
        };

        match declared {
            Some(project_machine) if project_machine == machine => {
                Ok(ProjectPlacement::Member { project })
            }
            Some(project_machine) => Err(miette!(
                "the template declares state machine '{machine}', but the Panta project at {} \
                 is governed by '{project_machine}'. One state machine governs a whole project \
                 (§FS-rhei-panta.6), so this rhei could never load there. Instantiate it as its \
                 own project instead — `--output {}` — or use a template built for \
                 '{project_machine}'.",
                project.display(),
                sibling_output_suggestion(&project, output_dir).display()
            )),
            None => {
                // Nothing declared: the project runs the built-in default, and
                // so does every rhei already in it. Adopting the template's
                // machine is safe only while no sibling depends on the default.
                let siblings = sibling_rhei_machines(&project, output_dir);
                let conflicting: Vec<&(String, Option<String>)> = siblings
                    .iter()
                    .filter(|(_, declared)| declared.as_deref() != Some(machine))
                    .collect();
                if conflicting.is_empty() {
                    Ok(ProjectPlacement::Adopts { project, machine: machine.to_string() })
                } else {
                    let names = conflicting
                        .iter()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    Err(miette!(
                        "the template declares state machine '{machine}', but the Panta project \
                         at {} already holds rheis on the built-in default machine ({names}). \
                         One state machine governs a whole project (§FS-rhei-panta.6), so \
                         adopting '{machine}' would break them. Instantiate it as its own \
                         project instead: `--output {}`.",
                        project.display(),
                        sibling_output_suggestion(&project, output_dir).display()
                    ))
                }
            }
        }
    }

    /// The rheis already in `project`, paired with the machine each declares,
    /// excluding the one being instantiated at `exclude` — by the time this
    /// runs the new workspace is already on disk, and counting it as a sibling
    /// makes every first template look like a collision with itself.
    fn sibling_rhei_machines(project: &Path, exclude: &Path) -> Vec<(String, Option<String>)> {
        let excluded = exclude.canonicalize().unwrap_or_else(|_| exclude.to_path_buf());
        let Ok(entries) = workspace::discover_rhei_entries(project) else {
            return Vec::new();
        };
        entries
            .into_iter()
            .filter(|entry| {
                entry.canonicalize().unwrap_or_else(|_| entry.clone()) != excluded
            })
            .map(|entry| {
                let id = entry
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("?")
                    .trim_end_matches(".rhei")
                    .to_string();
                let declared = workspace_declared_machine(&entry);
                (id, declared)
            })
            .collect()
    }

    /// A path outside the project, next to it, keeping the chosen directory name.
    fn sibling_output_suggestion(project: &Path, output_dir: &Path) -> PathBuf {
        let name = output_dir.file_name().unwrap_or_default();
        match project.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
            _ => PathBuf::from(name),
        }
    }

    /// The state machine `index.panta.md` declares, if it declares one.
    fn project_declared_machine(project: &Path) -> MietteResult<Option<String>> {
        let manifest = project.join("index.panta.md");
        let content = fs::read_to_string(&manifest)
            .map_err(|err| file_io_report(&manifest, "failed to read the project manifest", err))?;
        let parsed = rhei_core::parser::parse_panta_manifest(&content)
            .map_err(|err| miette!("failed to parse {}: {err:?}", manifest.display()))?;
        Ok(parsed.states_declared.then_some(parsed.states))
    }

    /// The state machine a materialized rhei declares, if it declares one.
    ///
    /// A Directory Workspace keeps its tickets in `tasks/`, so its
    /// `index.rhei.md` does not parse as a standalone plan — reading it that
    /// way reported "declares nothing" for every workspace template and skipped
    /// the placement check the moment it mattered most.
    pub(super) fn workspace_declared_machine(entrypoint: &Path) -> Option<String> {
        let rhei = match workspace::workspace_dir(entrypoint) {
            Some(dir) => workspace::load_workspace(&dir).ok()?.rhei,
            None => rhei_core::parser::parse(&fs::read_to_string(entrypoint).ok()?).ok()?,
        };
        rhei.states_declared.then_some(rhei.states)
    }

    /// Write the adopted machine into `index.panta.md` so the project and its
    /// first rhei agree. Mirrors what `rhei init` does when it adopts a machine
    /// from plans already on disk. §FS-rhei-init.2
    pub(super) fn adopt_project_machine(project: &Path, machine: &str) -> MietteResult<()> {
        let manifest = project.join("index.panta.md");
        let content = fs::read_to_string(&manifest)
            .map_err(|err| file_io_report(&manifest, "failed to read the project manifest", err))?;
        let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
        let states_line = format!("**States:** {machine}");
        if let Some(existing) = lines.iter_mut().find(|line| line.starts_with("**States:**")) {
            *existing = states_line;
        } else {
            // The declaration belongs directly under the `# Panta:` heading.
            let insert_at = lines
                .iter()
                .position(|line| line.starts_with("# Panta:"))
                .map(|idx| idx + 1)
                .unwrap_or(lines.len());
            lines.insert(insert_at, states_line);
        }
        let mut rendered = lines.join("\n");
        rendered.push('\n');
        fs::write(&manifest, rendered)
            .map_err(|err| file_io_report(&manifest, "failed to write the project manifest", err))
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
