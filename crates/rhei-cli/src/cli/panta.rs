// Panta orchestration: the project recipe and its commands. §FS-rhei-panta.7

/// One rhei entry in the project recipe (`rheis:` in `index.panta.md`
/// frontmatter). §FS-rhei-panta.7.1
#[derive(Debug, Clone, Deserialize)]
struct RecipeEntry {
    id: String,
    #[serde(default)]
    template: String,
    #[serde(default)]
    inputs: YamlMapping,
    #[serde(default, rename = "depends-on")]
    depends_on: Vec<String>,
}

const PANTA_INDEX_FILE: &str = "index.panta.md";
const RESERVED_RHEI_IDS: [&str; 2] = ["basin", "panta"];

/// Resolve the Panta project directory from `--project` or the cwd / ancestors.
/// §FS-rhei-panta.6
fn resolve_panta_project_dir(explicit: Option<&Path>) -> MietteResult<PathBuf> {
    if let Some(path) = explicit {
        let dir = if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(PANTA_INDEX_FILE)
        {
            path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
        } else {
            path.to_path_buf()
        };
        if !dir.join(PANTA_INDEX_FILE).is_file() {
            return Err(miette!(
                "'{}' is not a Panta project (no {PANTA_INDEX_FILE})",
                dir.display()
            ));
        }
        return Ok(dir);
    }

    let cwd = std::env::current_dir()
        .map_err(|err| miette!("failed to determine working directory: {err}"))?;
    let mut dir = Some(cwd.as_path());
    while let Some(d) = dir {
        if d.join(PANTA_INDEX_FILE).is_file() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(miette!(
        "no Panta project found: {PANTA_INDEX_FILE} not present in the current directory or any parent. Pass --project <dir>."
    ))
}

/// Split a manifest into (head, optional frontmatter YAML, body). The head holds
/// the `# Panta:` header and any `**States:**` line; frontmatter is the YAML
/// between the `---` fences that follow the header. §FS-rhei-panta.7.1
fn split_manifest(content: &str) -> (Vec<String>, Option<String>, Vec<String>) {
    let lines: Vec<String> = content.lines().map(str::to_string).collect();

    // The frontmatter fence is the first `---` line preceded only by the header,
    // a `**States:**` line, and blank lines (mirrors the plan parser rule).
    let mut fence_open: Option<usize> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "---" {
            let only_header_before = lines[..idx].iter().all(|l| {
                let lt = l.trim();
                lt.is_empty() || lt.starts_with("# Panta:") || lt.starts_with("**States:**")
            });
            if only_header_before {
                fence_open = Some(idx);
            }
            break;
        }
        if trimmed.starts_with("##") {
            break;
        }
    }

    if let Some(open) = fence_open {
        if let Some(rel_close) =
            lines[open + 1..].iter().position(|l| l.trim() == "---")
        {
            let close = open + 1 + rel_close;
            let head = lines[..open].to_vec();
            let frontmatter = lines[open + 1..close].join("\n");
            let body = lines[close + 1..].to_vec();
            return (head, Some(frontmatter), body);
        }
    }

    // No frontmatter: head is the header plus any leading `**States:**`/blank
    // lines; everything from the first content line onward is the body.
    let mut head_end = 0;
    for (idx, line) in lines.iter().enumerate() {
        let lt = line.trim();
        if lt.starts_with("# Panta:") || lt.starts_with("**States:**") || lt.is_empty() {
            head_end = idx + 1;
        } else {
            break;
        }
    }
    let head = lines[..head_end].to_vec();
    let body = lines[head_end..].to_vec();
    (head, None, body)
}

/// Parse the `rheis:` recipe out of a manifest's frontmatter. §FS-rhei-panta.7.1
fn read_recipe(manifest_path: &Path) -> MietteResult<Vec<RecipeEntry>> {
    let content = fs::read_to_string(manifest_path).map_err(|err| {
        miette!("failed to read Panta manifest '{}': {err}", manifest_path.display())
    })?;
    let (_, frontmatter, _) = split_manifest(&content);
    let Some(frontmatter) = frontmatter else {
        return Ok(Vec::new());
    };
    let mapping: YamlMapping = if frontmatter.trim().is_empty() {
        YamlMapping::new()
    } else {
        serde_yaml::from_str(&frontmatter).map_err(|err| {
            miette!("failed to parse frontmatter of '{}': {err}", manifest_path.display())
        })?
    };
    let Some(rheis) = mapping.get(YamlValue::from("rheis")) else {
        return Ok(Vec::new());
    };
    let entries: Vec<RecipeEntry> = serde_yaml::from_value(rheis.clone()).map_err(|err| {
        miette!("invalid `rheis:` recipe in '{}': {err}", manifest_path.display())
    })?;
    Ok(entries)
}

fn is_valid_rhei_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Validate recipe integrity: unique valid ids, resolvable dependency targets,
/// and an acyclic dependency graph. §FS-rhei-panta.7.2
fn validate_recipe(entries: &[RecipeEntry]) -> MietteResult<()> {
    let mut ids = HashSet::new();
    for entry in entries {
        if !is_valid_rhei_id(&entry.id) {
            return Err(miette!(
                "invalid rhei id '{}': must start with a letter and contain only letters, digits, '-' or '_'",
                entry.id
            ));
        }
        if RESERVED_RHEI_IDS.contains(&entry.id.as_str()) {
            return Err(miette!("rhei id '{}' is reserved and cannot be used", entry.id));
        }
        if !ids.insert(entry.id.as_str()) {
            return Err(miette!("duplicate rhei id '{}' in the recipe", entry.id));
        }
    }
    for entry in entries {
        for dep in &entry.depends_on {
            if !ids.contains(dep.as_str()) {
                return Err(miette!(
                    "rhei '{}' depends-on '{}', which is not a recipe entry",
                    entry.id,
                    dep
                ));
            }
        }
    }
    detect_recipe_cycle(entries)?;
    Ok(())
}

/// Reject a cyclic rhei dependency graph with the cycle path. §FS-rhei-panta.7.2
fn detect_recipe_cycle(entries: &[RecipeEntry]) -> MietteResult<()> {
    let deps: HashMap<&str, &Vec<String>> =
        entries.iter().map(|e| (e.id.as_str(), &e.depends_on)).collect();

    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    let mut marks: HashMap<&str, Mark> = HashMap::new();

    fn visit<'a>(
        id: &'a str,
        deps: &HashMap<&'a str, &'a Vec<String>>,
        marks: &mut HashMap<&'a str, Mark>,
        stack: &mut Vec<&'a str>,
    ) -> MietteResult<()> {
        match marks.get(id) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Visiting) => {
                stack.push(id);
                let cycle = stack.join(" -> ");
                return Err(miette!("rhei dependency cycle: {cycle}"));
            }
            None => {}
        }
        marks.insert(id, Mark::Visiting);
        stack.push(id);
        if let Some(targets) = deps.get(id) {
            for target in targets.iter() {
                if let Some((dep_id, _)) = deps.get_key_value(target.as_str()) {
                    visit(dep_id, deps, marks, stack)?;
                }
            }
        }
        stack.pop();
        marks.insert(id, Mark::Done);
        Ok(())
    }

    for entry in entries {
        let mut stack = Vec::new();
        visit(entry.id.as_str(), &deps, &mut marks, &mut stack)?;
    }
    Ok(())
}

/// `rhei panta add` — append a rhei entry to the recipe manifest. §FS-rhei-panta.7.3
pub(crate) fn panta_add_command(
    id: &str,
    template: &str,
    set_values: &[String],
    depends_on: &[String],
    project: Option<&Path>,
) -> MietteResult<()> {
    let project_dir = resolve_panta_project_dir(project)?;
    let manifest_path = project_dir.join(PANTA_INDEX_FILE);

    // Validate the new entry against the current recipe before writing.
    let mut entries = read_recipe(&manifest_path)?;
    if !is_valid_rhei_id(id) {
        return Err(miette!(
            "invalid rhei id '{id}': must start with a letter and contain only letters, digits, '-' or '_'"
        ));
    }
    if RESERVED_RHEI_IDS.contains(&id) {
        return Err(miette!("rhei id '{id}' is reserved and cannot be used"));
    }
    if entries.iter().any(|e| e.id == id) {
        return Err(miette!("rhei id '{id}' already exists in the recipe"));
    }
    for dep in depends_on {
        if !entries.iter().any(|e| &e.id == dep) {
            return Err(miette!("--depends-on '{dep}' is not an existing recipe entry"));
        }
    }
    // The template must resolve like `rhei instantiate`.
    templates::resolve_template_reference(template)?;

    let inputs = parse_set_values(set_values)?;

    // Build the new entry and confirm the recipe stays valid (acyclicity, etc.).
    let new_entry = RecipeEntry {
        id: id.to_string(),
        template: template.to_string(),
        inputs: inputs.clone(),
        depends_on: depends_on.to_vec(),
    };
    entries.push(new_entry);
    validate_recipe(&entries)?;

    write_recipe_entry(&manifest_path, id, template, &inputs, depends_on)?;

    println!("Added rhei '{id}' to the recipe at '{}'.", manifest_path.display());
    if depends_on.is_empty() {
        println!("  template: {template}  (no dependencies)");
    } else {
        println!("  template: {template}  depends-on: {}", depends_on.join(", "));
    }
    println!();
    println!("Run the project with:");
    println!("  rhei panta");
    Ok(())
}

/// Order recipe entries so every rhei comes after the rheis it depends on.
/// The recipe is already validated acyclic, so a stable topological sort exists.
/// §FS-rhei-panta.7.4
fn topological_order(entries: &[RecipeEntry]) -> Vec<usize> {
    let index_of: HashMap<&str, usize> =
        entries.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut visited = vec![false; entries.len()];
    let mut order = Vec::with_capacity(entries.len());

    fn visit(
        i: usize,
        entries: &[RecipeEntry],
        index_of: &HashMap<&str, usize>,
        visited: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if visited[i] {
            return;
        }
        visited[i] = true;
        for dep in &entries[i].depends_on {
            if let Some(&j) = index_of.get(dep.as_str()) {
                visit(j, entries, index_of, visited, order);
            }
        }
        order.push(i);
    }

    for i in 0..entries.len() {
        visit(i, entries, &index_of, &mut visited, &mut order);
    }
    order
}

/// Restrict the recipe to the named rheis and their transitive dependencies.
/// §FS-rhei-panta.7.4
fn restrict_to_rheis(entries: &[RecipeEntry], only: &[String]) -> MietteResult<Vec<usize>> {
    let index_of: HashMap<&str, usize> =
        entries.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut keep = HashSet::new();
    let mut stack: Vec<usize> = Vec::new();
    for name in only {
        let &i = index_of.get(name.as_str()).ok_or_else(|| {
            miette!("--rhei '{name}' is not a recipe entry")
        })?;
        stack.push(i);
    }
    while let Some(i) = stack.pop() {
        if !keep.insert(i) {
            continue;
        }
        for dep in &entries[i].depends_on {
            if let Some(&j) = index_of.get(dep.as_str()) {
                stack.push(j);
            }
        }
    }
    Ok(topological_order(entries).into_iter().filter(|i| keep.contains(i)).collect())
}

/// Choose the next `runtime/panta-<n>/` run directory (monotonic). §FS-rhei-panta.7.5
fn allocate_run_dir(project_dir: &Path) -> MietteResult<(PathBuf, u64)> {
    let runtime = project_dir.join("runtime");
    let mut max_n = 0u64;
    if runtime.is_dir() {
        for entry in fs::read_dir(&runtime)
            .map_err(|err| miette!("failed to read '{}': {err}", runtime.display()))?
        {
            let entry = entry.map_err(|err| miette!("failed to read runtime entry: {err}"))?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(rest) = name.strip_prefix("panta-") {
                    if let Ok(n) = rest.parse::<u64>() {
                        max_n = max_n.max(n);
                    }
                }
            }
        }
    }
    let n = max_n + 1;
    let dir = runtime.join(format!("panta-{n}"));
    fs::create_dir_all(&dir)
        .map_err(|err| miette!("failed to create run directory '{}': {err}", dir.display()))?;
    Ok((dir, n))
}

/// Render a recipe entry's `inputs:` map as `key=value` strings for instantiation.
fn inputs_to_set_values(entry: &RecipeEntry) -> MietteResult<Vec<String>> {
    let mut out = Vec::new();
    for (key, value) in &entry.inputs {
        let key = key.as_str().ok_or_else(|| {
            miette!("rhei '{}' has a non-string input key", entry.id)
        })?;
        let rendered = match value {
            YamlValue::String(s) => s.clone(),
            YamlValue::Bool(b) => b.to_string(),
            YamlValue::Number(n) => n.to_string(),
            _ => {
                return Err(miette!(
                    "rhei '{}' input '{key}' must be a scalar value",
                    entry.id
                ))
            }
        };
        out.push(format!("{key}={rendered}"));
    }
    Ok(out)
}

fn instance_entrypoint(instance_dir: &Path) -> MietteResult<PathBuf> {
    for candidate in ["index.rhei.md", "plan.rhei.md"] {
        let path = instance_dir.join(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(miette!(
        "instantiated rhei at '{}' has no index.rhei.md or plan.rhei.md",
        instance_dir.display()
    ))
}

/// `rhei panta` — instantiate and run the recipe in dependency order.
/// §FS-rhei-panta.7.4
pub(crate) fn panta_run_command(
    only: &[String],
    dry_run: bool,
    project: Option<&Path>,
) -> MietteResult<()> {
    let project_dir = resolve_panta_project_dir(project)?;
    let manifest_path = project_dir.join(PANTA_INDEX_FILE);
    let entries = read_recipe(&manifest_path)?;
    validate_recipe(&entries)?;

    if entries.is_empty() {
        return Err(miette!(
            "recipe at '{}' has no rheis; add one with `rhei panta add`",
            manifest_path.display()
        ));
    }

    let order = if only.is_empty() {
        topological_order(&entries)
    } else {
        restrict_to_rheis(&entries, only)?
    };

    let (run_dir, run_id) = allocate_run_dir(&project_dir)?;

    // Report scope before doing any work. §FS-rhei-panta.6
    println!(
        "panta run #{run_id}: {} rhei(s) into '{}'{}",
        order.len(),
        run_dir.display(),
        if dry_run { " (dry run)" } else { "" }
    );
    for &i in &order {
        let entry = &entries[i];
        if entry.depends_on.is_empty() {
            println!("  - {} ({})", entry.id, entry.template);
        } else {
            println!("  - {} ({}) after {}", entry.id, entry.template, entry.depends_on.join(", "));
        }
    }
    println!();

    for &i in &order {
        let entry = &entries[i];
        let instance_dir = run_dir.join(&entry.id);
        let set_values = inputs_to_set_values(entry)?;

        println!("=== rhei '{}' ===", entry.id);
        // Instantiate the template into runtime/panta-<n>/<id>/. §FS-rhei-panta.7.4
        templates::instantiate_command(
            Some(&entry.template),
            &[],
            &[],
            &set_values,
            &[],
            &[],
            Some(&instance_dir),
            false,
            false,
            false,
            false,
        )?;

        if dry_run {
            continue;
        }

        let entrypoint = instance_entrypoint(&instance_dir)?;
        run_command(&entrypoint, None, default_run_options())?;
    }

    if dry_run {
        println!("Dry run OK: instantiated {} rhei(s) under '{}'.", order.len(), run_dir.display());
    } else {
        println!("panta run #{run_id} complete: {} rhei(s) executed.", order.len());
    }
    Ok(())
}

fn parse_set_values(set_values: &[String]) -> MietteResult<YamlMapping> {
    let mut mapping = YamlMapping::new();
    for raw in set_values {
        let (key, value) = raw
            .split_once('=')
            .ok_or_else(|| miette!("--set expects KEY=VALUE, got '{raw}'"))?;
        mapping.insert(YamlValue::from(key.to_string()), YamlValue::from(value.to_string()));
    }
    Ok(mapping)
}

/// Append a recipe entry to the manifest's `rheis:` frontmatter list, preserving
/// the header, body, and existing entries. §FS-rhei-panta.7.3
fn write_recipe_entry(
    manifest_path: &Path,
    id: &str,
    template: &str,
    inputs: &YamlMapping,
    depends_on: &[String],
) -> MietteResult<()> {
    let content = fs::read_to_string(manifest_path).map_err(|err| {
        miette!("failed to read Panta manifest '{}': {err}", manifest_path.display())
    })?;
    let (head, frontmatter, body) = split_manifest(&content);

    let mut mapping: YamlMapping = match &frontmatter {
        Some(fm) if !fm.trim().is_empty() => serde_yaml::from_str(fm).map_err(|err| {
            miette!("failed to parse frontmatter of '{}': {err}", manifest_path.display())
        })?,
        _ => YamlMapping::new(),
    };

    let mut entry = YamlMapping::new();
    entry.insert(YamlValue::from("id"), YamlValue::from(id.to_string()));
    entry.insert(YamlValue::from("template"), YamlValue::from(template.to_string()));
    if !inputs.is_empty() {
        entry.insert(YamlValue::from("inputs"), YamlValue::Mapping(inputs.clone()));
    }
    if !depends_on.is_empty() {
        let deps = depends_on.iter().map(|d| YamlValue::from(d.clone())).collect();
        entry.insert(YamlValue::from("depends-on"), YamlValue::Sequence(deps));
    }

    let rheis_key = YamlValue::from("rheis");
    let mut sequence = match mapping.get(&rheis_key) {
        Some(YamlValue::Sequence(seq)) => seq.clone(),
        Some(YamlValue::Null) | None => Vec::new(),
        Some(_) => {
            return Err(miette!(
                "`rheis:` in '{}' is not a list",
                manifest_path.display()
            ))
        }
    };
    sequence.push(YamlValue::Mapping(entry));
    mapping.insert(rheis_key, YamlValue::Sequence(sequence));

    let frontmatter_yaml = serde_yaml::to_string(&mapping)
        .map_err(|err| miette!("failed to serialize recipe frontmatter: {err}"))?;

    let mut out = String::new();
    for line in &head {
        out.push_str(line);
        out.push('\n');
    }
    // Ensure a blank line separates the header from the frontmatter fence.
    if !head.is_empty() && head.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(frontmatter_yaml.trim_end());
    out.push('\n');
    out.push_str("---\n");
    for line in &body {
        out.push_str(line);
        out.push('\n');
    }

    fs::write(manifest_path, out).map_err(|err| {
        miette!("failed to write Panta manifest '{}': {err}", manifest_path.display())
    })?;
    Ok(())
}
