    fn parse_template_source_filter(value: &str) -> MietteResult<TemplateSourceFilter> {
        match value.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(TemplateSourceFilter::Project),
            "user" => Ok(TemplateSourceFilter::User),
            "all" => Ok(TemplateSourceFilter::All),
            other => Err(miette!(
                "invalid template source '{}'. Expected one of: project, user, all",
                other
            )),
        }
    }

    fn discover_templates(filter: TemplateSourceFilter) -> MietteResult<Vec<DiscoveredTemplate>> {
        let mut templates = Vec::new();
        let mut seen = HashSet::new();

        for (source, root) in template_search_roots(filter)? {
            if !root.is_dir() {
                continue;
            }

            let mut entries = fs::read_dir(&root)
                .map_err(|err| file_io_report(&root, "failed to read template directory", err))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| {
                    miette!("failed to read dir entry in '{}': {err}", root.display())
                })?;
            entries.sort_by_key(|entry| entry.file_name());

            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || !path.is_dir() || seen.contains(&name) {
                    continue;
                }

                let Ok(manifest) = load_template_manifest(&path) else {
                    continue;
                };

                seen.insert(name);
                templates.push(DiscoveredTemplate { manifest, path, source });
            }
        }

        Ok(templates)
    }

    fn template_search_roots(
        filter: TemplateSourceFilter,
    ) -> MietteResult<Vec<(TemplateSource, PathBuf)>> {
        let mut roots = Vec::new();

        if filter.includes(TemplateSource::Project) {
            roots.push((
                TemplateSource::Project,
                find_project_root()?.join(".agents").join("rhei").join("templates"),
            ));
        }
        if filter.includes(TemplateSource::User) {
            roots.push((
                TemplateSource::User,
                home_dir()?.join(".agents").join("rhei").join("templates"),
            ));
        }

        Ok(roots)
    }

    pub(super) fn resolve_template_reference(reference: &str) -> MietteResult<PathBuf> {
        if template_reference_is_path(reference) {
            let path = PathBuf::from(reference);
            if !path.is_dir() {
                return Err(miette!("template directory '{}' does not exist", path.display()));
            }
            return Ok(path);
        }

        for (_, root) in template_search_roots(TemplateSourceFilter::All)? {
            let candidate = root.join(reference);
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }

        let suggestion = closest_template_name(reference)?;
        if let Some(name) = suggestion {
            // §FS-rhei-templates.6.1.2: named-template lookup reports a close discovered match.
            return Err(miette!(
                "template '{}' not found in project or user template directories. Did you mean '{}'?",
                reference,
                name
            ));
        }

        Err(miette!("template '{}' not found in project or user template directories", reference))
    }

    fn template_reference_is_path(reference: &str) -> bool {
        let path = Path::new(reference);
        path.is_absolute() || reference.contains('/') || reference.starts_with('.')
    }

    fn closest_template_name(reference: &str) -> MietteResult<Option<String>> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Ok(None);
        }

        let closest = discover_templates(TemplateSourceFilter::All)?
            .into_iter()
            .map(|template| {
                let name = template.manifest.name;
                let distance = template_name_distance(reference, &name);
                (name, distance)
            })
            .min_by(|(left_name, left_distance), (right_name, right_distance)| {
                left_distance.cmp(right_distance).then_with(|| left_name.cmp(right_name))
            });

        let Some((name, distance)) = closest else {
            return Ok(None);
        };

        let threshold = std::cmp::max(2, reference.chars().count() / 3);
        Ok((distance <= threshold).then_some(name))
    }

    fn template_name_distance(left: &str, right: &str) -> usize {
        let left = left.to_ascii_lowercase();
        let right = right.to_ascii_lowercase();
        levenshtein_distance(&left, &right)
    }

    fn levenshtein_distance(left: &str, right: &str) -> usize {
        if left == right {
            return 0;
        }
        if left.is_empty() {
            return right.chars().count();
        }
        if right.is_empty() {
            return left.chars().count();
        }

        let right_chars = right.chars().collect::<Vec<_>>();
        let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
        let mut current = vec![0; right_chars.len() + 1];

        for (left_index, left_char) in left.chars().enumerate() {
            current[0] = left_index + 1;
            for (right_index, right_char) in right_chars.iter().enumerate() {
                let insertion = current[right_index] + 1;
                let deletion = previous[right_index + 1] + 1;
                let substitution = previous[right_index] + usize::from(left_char != *right_char);
                current[right_index + 1] = insertion.min(deletion).min(substitution);
            }
            std::mem::swap(&mut previous, &mut current);
        }

        previous[right_chars.len()]
    }

    fn load_template_manifest(template_dir: &Path) -> MietteResult<TemplateManifest> {
        let manifest_path = template_dir.join("template.yaml");
        let raw = fs::read_to_string(&manifest_path).map_err(|err| {
            file_io_report(&manifest_path, "failed to read template manifest", err)
        })?;
        let manifest: TemplateManifest = serde_yaml::from_str(&raw)
            .map_err(|err| miette!("failed to parse '{}': {err}", manifest_path.display()))?;
        validate_template_manifest(&manifest, template_dir)?;
        Ok(manifest)
    }

    fn validate_template_manifest(
        manifest: &TemplateManifest,
        template_dir: &Path,
    ) -> MietteResult<()> {
        let dir_name =
            template_dir.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
                miette!("template path '{}' has no directory name", template_dir.display())
            })?;
        let ident = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$")
            .expect("template identifier regex should be valid");

        if manifest.name != dir_name {
            return Err(miette!(
                "template manifest name '{}' does not match directory '{}'",
                manifest.name,
                dir_name
            ));
        }
        if !ident.is_match(&manifest.name) {
            return Err(miette!("template name '{}' is not a valid identifier", manifest.name));
        }
        if manifest.description.trim().is_empty() {
            return Err(miette!(
                "template '{}' must include a non-empty description",
                manifest.name
            ));
        }

        let cwd = std::env::current_dir()
            .map_err(|err| miette!("failed to determine working directory: {err}"))?;
        let mut seen = HashSet::new();
        let mut positional_indexes = Vec::new();

        for input in &manifest.inputs {
            if !ident.is_match(&input.name) {
                return Err(miette!(
                    "template '{}' input '{}' is not a valid identifier",
                    manifest.name,
                    input.name
                ));
            }
            if !seen.insert(input.name.as_str()) {
                return Err(miette!(
                    "template '{}' declares duplicate input '{}'",
                    manifest.name,
                    input.name
                ));
            }
            if input.description.trim().is_empty() {
                return Err(miette!(
                    "template '{}' input '{}' must include a description",
                    manifest.name,
                    input.name
                ));
            }
            if let Some(index) = input.positional {
                if index == 0 {
                    return Err(miette!(
                        "template '{}' input '{}' positional index must be >= 1",
                        manifest.name,
                        input.name
                    ));
                }
                positional_indexes.push((index, input.name.as_str()));
            }
            if input.schema.required == Some(true) && input.schema.default.is_some() {
                return Err(miette!(
                    "template '{}' input '{}' cannot set both required: true and default",
                    manifest.name,
                    input.name
                ));
            }
            validate_template_value_schema(&manifest.name, &input.name, &input.schema)?;
            if let Some(default) = input.schema.default.as_ref() {
                let _ = coerce_template_input_value(input, default, &cwd, true)?;
            }
        }

        positional_indexes.sort_by_key(|(index, _)| *index);
        for (expected, (actual, name)) in positional_indexes.iter().enumerate() {
            let expected = expected + 1;
            if *actual != expected {
                return Err(miette!(
                    "template '{}' input '{}' declares positional {}, but positional indexes must be unique and contiguous starting at 1",
                    manifest.name,
                    name,
                    actual
                ));
            }
        }

        let _ = detect_template_layout(template_dir)?;

        Ok(())
    }

    fn validate_template_value_schema(
        template_name: &str,
        label: &str,
        schema: &TemplateValueSchema,
    ) -> MietteResult<()> {
        if let Some(pattern) = schema.validate.as_deref() {
            if matches!(schema.value_type, TemplateInputType::Array | TemplateInputType::Object) {
                return Err(miette!(
                    "template '{}' input '{}' cannot set validate on {} values",
                    template_name,
                    label,
                    schema.value_type.as_str()
                ));
            }
            let _ = compile_full_match_regex(pattern).map_err(|err| {
                miette!(
                    "template '{}' input '{}' has invalid validate regex: {err}",
                    template_name,
                    label
                )
            })?;
        }

        match schema.value_type {
            TemplateInputType::Array => {
                let Some(items) = schema.items.as_deref() else {
                    return Err(miette!(
                        "template '{}' input '{}' with type array must declare items",
                        template_name,
                        label
                    ));
                };
                if !schema.properties.is_empty() {
                    return Err(miette!(
                        "template '{}' input '{}' with type array cannot declare properties",
                        template_name,
                        label
                    ));
                }
                validate_template_value_schema(template_name, label, items)?;
            }
            TemplateInputType::Object => {
                if schema.items.is_some() {
                    return Err(miette!(
                        "template '{}' input '{}' with type object cannot declare items",
                        template_name,
                        label
                    ));
                }
                for (property, property_schema) in &schema.properties {
                    validate_template_value_schema(
                        template_name,
                        &format!("{label}.{property}"),
                        property_schema,
                    )?;
                }
            }
            _ => {
                if schema.items.is_some() {
                    return Err(miette!(
                        "template '{}' input '{}' with type {} cannot declare items",
                        template_name,
                        label,
                        schema.value_type.as_str()
                    ));
                }
                if !schema.properties.is_empty() {
                    return Err(miette!(
                        "template '{}' input '{}' with type {} cannot declare properties",
                        template_name,
                        label,
                        schema.value_type.as_str()
                    ));
                }
            }
        }

        Ok(())
    }

    fn detect_template_layout(template_dir: &Path) -> MietteResult<TemplateLayout> {
        let plan_path = template_dir.join("plan.rhei.md");
        let index_path = template_dir.join("index.rhei.md");
        let has_plan = plan_path.is_file();
        let has_index = index_path.is_file();

        match (has_plan, has_index) {
            (true, false) => Ok(TemplateLayout::SingleFile),
            (false, true) => {
                let tasks_dir = template_dir.join("tasks");
                if !tasks_dir.is_dir() {
                    return Err(miette!(
                        "template '{}' is a workspace template but is missing tasks/",
                        template_dir.display()
                    ));
                }
                Ok(TemplateLayout::Workspace)
            }
            (true, true) => Err(miette!(
                "template '{}' contains both plan.rhei.md and index.rhei.md",
                template_dir.display()
            )),
            (false, false) => Err(miette!(
                "template '{}' must contain either plan.rhei.md or index.rhei.md",
                template_dir.display()
            )),
        }
    }

    fn collect_template_inputs(
        manifest: &TemplateManifest,
        values_files: &[PathBuf],
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
    ) -> MietteResult<BTreeMap<String, serde_json::Value>> {
        let cwd = std::env::current_dir()
            .map_err(|err| miette!("failed to determine working directory: {err}"))?;
        let mut raw_values: BTreeMap<String, YamlValue> = BTreeMap::new();

        for values_file in values_files {
            let loaded = load_template_values_file(values_file)?;
            for (key, value) in loaded {
                raw_values.insert(key, value);
            }
        }

        let parsed_input_args = parse_template_input_args(manifest, input_args)?;
        for (key, value) in parsed_input_args.positional_values {
            raw_values.insert(key, YamlValue::String(value));
        }
        for (key, value) in parsed_input_args.assignments {
            raw_values.insert(key, YamlValue::String(value));
        }

        for assignment in set_values {
            let (key, value) = parse_assignment(assignment, "--set")?;
            raw_values.insert(key, YamlValue::String(value));
        }

        for assignment in set_files {
            let (key, value_path) = parse_assignment(assignment, "--set-file")?;
            let path = PathBuf::from(value_path);
            let contents = fs::read_to_string(&path)
                .map_err(|err| file_io_report(&path, "failed to read --set-file input", err))?;
            raw_values.insert(key, YamlValue::String(contents));
        }

        let declared_inputs =
            manifest.inputs.iter().map(|input| input.name.as_str()).collect::<HashSet<_>>();
        for key in raw_values.keys() {
            if !declared_inputs.contains(key.as_str()) {
                return Err(miette!(
                    "template '{}' does not declare an input named '{}'",
                    manifest.name,
                    key
                ));
            }
        }

        let mut resolved = BTreeMap::new();
        for input in &manifest.inputs {
            let value = if let Some(raw) = raw_values.get(&input.name) {
                coerce_template_input_value(input, raw, &cwd, false)?
            } else if let Some(default) = input.schema.default.as_ref() {
                coerce_template_input_value(input, default, &cwd, true)?
            } else if input.is_required() {
                return Err(miette!(
                    "template '{}' requires input '{}'",
                    manifest.name,
                    input.name
                ));
            } else {
                empty_template_value(&input.schema)
            };

            validate_resolved_value(&input.name, &input.schema, &value)?;

            resolved.insert(input.name.clone(), value);
        }

        Ok(resolved)
    }

    /// Enforce each scalar `validate` pattern in `schema` against the matching
    /// scalar in the already-coerced `value`, recursing through array items and
    /// object properties. This is what makes `validate` declared on a nested
    /// `properties.<x>` or array `items` scalar take effect — not only on
    /// top-level inputs. Patterns are guaranteed valid here because
    /// `validate_template_value_schema` compiled them at manifest-load time.
    fn validate_resolved_value(
        label: &str,
        schema: &TemplateValueSchema,
        value: &serde_json::Value,
    ) -> MietteResult<()> {
        if let Some(pattern) = schema.validate.as_deref() {
            let regex = compile_full_match_regex(pattern)
                .map_err(|err| miette!("input '{}' has invalid validate regex: {err}", label))?;
            let rendered = scalar_template_value_as_string(value).ok_or_else(|| {
                miette!(
                    "input '{}' uses validate but did not resolve to a scalar string value",
                    label
                )
            })?;
            if !regex.is_match(&rendered) {
                return Err(miette!(
                    "input '{}' does not match validation pattern '{}'",
                    label,
                    pattern
                ));
            }
        }

        match schema.value_type {
            TemplateInputType::Array => {
                if let (Some(items), serde_json::Value::Array(elements)) =
                    (schema.items.as_deref(), value)
                {
                    for (idx, element) in elements.iter().enumerate() {
                        validate_resolved_value(&format!("{label}[{idx}]"), items, element)?;
                    }
                }
            }
            TemplateInputType::Object => {
                if let serde_json::Value::Object(map) = value {
                    for (property, property_schema) in &schema.properties {
                        if let Some(element) = map.get(property) {
                            validate_resolved_value(
                                &format!("{label}.{property}"),
                                property_schema,
                                element,
                            )?;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
