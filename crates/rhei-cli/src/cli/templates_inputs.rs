    #[derive(Debug, Default)]
    struct ParsedTemplateInputArgs {
        positional_values: Vec<(String, String)>,
        assignments: Vec<(String, String)>,
    }

    fn parse_template_input_args(
        manifest: &TemplateManifest,
        input_args: &[String],
    ) -> MietteResult<ParsedTemplateInputArgs> {
        let ident = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$")
            .expect("template identifier regex should be valid");
        let declared_inputs =
            manifest.inputs.iter().map(|input| input.name.as_str()).collect::<HashSet<_>>();
        let mut positional_values = Vec::new();
        let mut assignments = Vec::new();

        for value in input_args {
            if let Some((key, rhs)) = value.split_once('=') {
                if ident.is_match(key) {
                    if !declared_inputs.contains(key) {
                        return Err(miette!(
                            "template '{}' does not declare an input named '{}'",
                            manifest.name,
                            key
                        ));
                    }
                    assignments.push((key.to_string(), rhs.to_string()));
                    continue;
                }
            }

            positional_values.push(value.clone());
        }

        let positional_values = map_template_positional_inputs(manifest, &positional_values)?;
        Ok(ParsedTemplateInputArgs { positional_values, assignments })
    }

    fn map_template_positional_inputs(
        manifest: &TemplateManifest,
        values: &[String],
    ) -> MietteResult<Vec<(String, String)>> {
        if values.is_empty() {
            return Ok(Vec::new());
        }

        let positional_inputs = manifest
            .inputs
            .iter()
            .filter_map(|input| input.positional.map(|index| (index, input)))
            .collect::<BTreeMap<_, _>>();

        if !positional_inputs.is_empty() {
            let mut mapped = Vec::new();
            for (idx, value) in values.iter().enumerate() {
                let position = idx + 1;
                let Some(input) = positional_inputs.get(&position) else {
                    return Err(miette!(
                        "template '{}' does not declare positional input {}",
                        manifest.name,
                        position
                    ));
                };
                mapped.push((input.name.clone(), value.clone()));
            }
            return Ok(mapped);
        }

        let required =
            manifest.inputs.iter().filter(|input| input.is_required()).collect::<Vec<_>>();
        if required.len() == 1 && values.len() == 1 {
            return Ok(vec![(required[0].name.clone(), values[0].clone())]);
        }

        Err(miette!(
            "template '{}' does not accept positional inputs; use KEY=VALUE or --set",
            manifest.name
        ))
    }

    fn load_template_values_file(path: &Path) -> MietteResult<BTreeMap<String, YamlValue>> {
        let raw = fs::read_to_string(path)
            .map_err(|err| file_io_report(path, "failed to read values file", err))?;
        if raw.trim().is_empty() {
            return Ok(BTreeMap::new());
        }

        let value: YamlValue = serde_yaml::from_str(&raw)
            .map_err(|err| miette!("failed to parse values file '{}': {err}", path.display()))?;
        let mapping = match value {
            YamlValue::Mapping(mapping) => mapping,
            _ => {
                return Err(miette!(
                    "values file '{}' must contain a YAML or JSON object at the top level",
                    path.display()
                ))
            }
        };

        let mut values = BTreeMap::new();
        for (key, value) in mapping {
            let Some(key) = key.as_str() else {
                return Err(miette!("values file '{}' contains a non-string key", path.display()));
            };
            values.insert(key.to_string(), value);
        }

        Ok(values)
    }

    fn parse_assignment(value: &str, flag_name: &str) -> MietteResult<(String, String)> {
        let Some((key, value)) = value.split_once('=') else {
            return Err(miette!("{} expects KEY=VALUE, got '{}'", flag_name, value));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(miette!("{} expects a non-empty key", flag_name));
        }
        Ok((key.to_string(), value.to_string()))
    }

    fn compile_full_match_regex(pattern: &str) -> Result<Regex> {
        Regex::new(&format!(r"\A(?:{})\z", pattern)).context("compile regex")
    }

    fn coerce_template_input_value(
        input: &TemplateInputDef,
        raw: &YamlValue,
        cwd: &Path,
        from_default: bool,
    ) -> MietteResult<serde_json::Value> {
        coerce_template_value(&input.name, &input.schema, raw, cwd, from_default)
    }

    fn coerce_template_value(
        label: &str,
        schema: &TemplateValueSchema,
        raw: &YamlValue,
        cwd: &Path,
        from_default: bool,
    ) -> MietteResult<serde_json::Value> {
        let source = if from_default { "default value" } else { "input value" };

        let rendered = match schema.value_type {
            TemplateInputType::String => match raw {
                YamlValue::Null => serde_json::Value::String(String::new()),
                YamlValue::String(value) => serde_json::Value::String(value.clone()),
                _ => return Err(miette!("{} for '{}' must be a string", source, label)),
            },
            TemplateInputType::Number => match raw {
                YamlValue::Number(value) => serde_json::to_value(value)
                    .map_err(|err| miette!("failed to serialize number for '{}': {err}", label))?,
                YamlValue::String(value) => {
                    let trimmed = value.trim();
                    let number_re = Regex::new(r"^-?\d+(?:\.\d+)?$")
                        .expect("number validation regex should be valid");
                    if !number_re.is_match(trimmed) {
                        return Err(miette!("{} for '{}' must be a number", source, label));
                    }
                    let parsed: YamlValue = serde_yaml::from_str(trimmed).map_err(|err| {
                        miette!("{} for '{}' must be a number: {err}", source, label)
                    })?;
                    serde_json::to_value(parsed).map_err(|err| {
                        miette!("failed to serialize number for '{}': {err}", label)
                    })?
                }
                _ => return Err(miette!("{} for '{}' must be a number", source, label)),
            },
            TemplateInputType::Boolean => match raw {
                YamlValue::Bool(value) => serde_json::Value::Bool(*value),
                YamlValue::String(value) => match value.trim() {
                    "true" => serde_json::Value::Bool(true),
                    "false" => serde_json::Value::Bool(false),
                    _ => return Err(miette!("{} for '{}' must be true or false", source, label)),
                },
                _ => return Err(miette!("{} for '{}' must be true or false", source, label)),
            },
            TemplateInputType::Path => match raw {
                YamlValue::String(value) => {
                    if value.is_empty() {
                        return Err(miette!("{} for '{}' must not be empty", source, label));
                    }
                    let path = PathBuf::from(value);
                    let resolved = if path.is_absolute() { path } else { cwd.join(path) };
                    if !from_default && !resolved.exists() {
                        return Err(miette!(
                            "{} for '{}' refers to a path that does not exist: {}",
                            source,
                            label,
                            resolved.display()
                        ));
                    }
                    serde_json::Value::String(resolved.display().to_string())
                }
                _ => return Err(miette!("{} for '{}' must be a path string", source, label)),
            },
            TemplateInputType::Array => {
                let sequence = parse_template_sequence(label, raw, source)?;
                let item_schema = schema.items.as_deref().ok_or_else(|| {
                    miette!("{} for '{}' requires an items schema", source, label)
                })?;
                let mut items = Vec::with_capacity(sequence.len());
                for (idx, item) in sequence.iter().enumerate() {
                    items.push(coerce_template_value(
                        &format!("{label}[{idx}]"),
                        item_schema,
                        item,
                        cwd,
                        from_default,
                    )?);
                }
                serde_json::Value::Array(items)
            }
            TemplateInputType::Object => {
                let mapping = parse_template_mapping(label, raw, source)?;
                let mut object = serde_json::Map::new();
                for (key_value, value) in mapping {
                    let Some(key) = key_value.as_str() else {
                        return Err(miette!(
                            "{} for '{}' contains a non-string key",
                            source,
                            label
                        ));
                    };
                    let property_schema = schema.properties.get(key).ok_or_else(|| {
                        miette!("{} for '{}' contains unknown property '{}'", source, label, key)
                    })?;
                    object.insert(
                        key.to_string(),
                        coerce_template_value(key, property_schema, &value, cwd, from_default)?,
                    );
                }
                for (property, property_schema) in &schema.properties {
                    if object.contains_key(property) {
                        continue;
                    }
                    if let Some(default) = property_schema.default.as_ref() {
                        object.insert(
                            property.clone(),
                            coerce_template_value(property, property_schema, default, cwd, true)?,
                        );
                    } else if property_schema.is_required() {
                        return Err(miette!(
                            "{} for '{}' is missing required property '{}'",
                            source,
                            label,
                            property
                        ));
                    } else {
                        object.insert(property.clone(), empty_template_value(property_schema));
                    }
                }
                serde_json::Value::Object(object)
            }
        };

        Ok(rendered)
    }

    fn parse_template_sequence(
        label: &str,
        raw: &YamlValue,
        source: &str,
    ) -> MietteResult<Vec<YamlValue>> {
        match parse_structured_template_value(raw, "array", label, source)? {
            YamlValue::Sequence(values) => Ok(values),
            _ => Err(miette!("{} for '{}' must be an array", source, label)),
        }
    }

    fn parse_template_mapping(
        label: &str,
        raw: &YamlValue,
        source: &str,
    ) -> MietteResult<YamlMapping> {
        match parse_structured_template_value(raw, "object", label, source)? {
            YamlValue::Mapping(values) => Ok(values),
            _ => Err(miette!("{} for '{}' must be an object", source, label)),
        }
    }

    fn parse_structured_template_value(
        raw: &YamlValue,
        expected: &str,
        label: &str,
        source: &str,
    ) -> MietteResult<YamlValue> {
        match raw {
            YamlValue::String(text) => serde_yaml::from_str::<YamlValue>(text).map_err(|err| {
                miette!(
                    "{} for '{}' must be valid YAML or JSON {} syntax: {err}",
                    source,
                    label,
                    expected
                )
            }),
            other => Ok(other.clone()),
        }
    }

    fn empty_template_value(schema: &TemplateValueSchema) -> serde_json::Value {
        match schema.value_type {
            TemplateInputType::String | TemplateInputType::Path => {
                serde_json::Value::String(String::new())
            }
            TemplateInputType::Number | TemplateInputType::Boolean => serde_json::Value::Null,
            TemplateInputType::Array => serde_json::Value::Array(Vec::new()),
            TemplateInputType::Object => serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    fn scalar_template_value_as_string(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Null => Some(String::new()),
            serde_json::Value::Bool(value) => Some(value.to_string()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            serde_json::Value::String(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn print_template_inputs(manifest: &TemplateManifest) {
        println!("Template: {}", manifest.name);
        println!("Version: {}", manifest.version_string());
        println!("Description: {}", manifest.description);

        if manifest.inputs.is_empty() {
            println!("Inputs: none");
            return;
        }

        println!("Inputs:");
        for input in &manifest.inputs {
            let requirement = if input.is_required() {
                "required".to_string()
            } else if let Some(default) = input.schema.default.as_ref() {
                format!("default={}", format_version(default))
            } else {
                "optional".to_string()
            };
            println!("  {} ({}, {})", input.name, input.value_type().as_str(), requirement);
            println!("    {}", input.description);
            if let Some(pattern) = input.schema.validate.as_deref() {
                println!("    validate: {}", pattern);
            }
        }
    }

    fn materialize_template(
        template_dir: &Path,
        layout: TemplateLayout,
        output_dir: &Path,
        values: &BTreeMap<String, serde_json::Value>,
    ) -> MietteResult<MaterializedTemplate> {
        fs::create_dir_all(output_dir)
            .map_err(|err| file_io_report(output_dir, "failed to create output directory", err))?;
        let root_permissions = fs::metadata(template_dir)
            .map_err(|err| file_io_report(template_dir, "failed to read template metadata", err))?
            .permissions();
        fs::set_permissions(output_dir, root_permissions).map_err(|err| {
            file_io_report(output_dir, "failed to preserve output directory permissions", err)
        })?;

        materialize_template_dir(template_dir, output_dir, template_dir, values)?;

        Ok(MaterializedTemplate { layout, output_dir: output_dir.to_path_buf() })
    }

    fn materialize_template_dir(
        src_dir: &Path,
        dest_dir: &Path,
        template_root: &Path,
        values: &BTreeMap<String, serde_json::Value>,
    ) -> MietteResult<()> {
        let mut entries = fs::read_dir(src_dir)
            .map_err(|err| file_io_report(src_dir, "failed to read template directory", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!("failed to read dir entry in '{}': {err}", src_dir.display()))?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }

            let src_path = entry.path();
            if src_path == template_root.join("template.yaml") {
                continue;
            }

            // §FS-rhei-templates.6.1.2: root settings become project settings
            // in the agent config tree; non-root `settings.json` stays put.
            let at_template_root = src_dir == template_root;
            let dest_path = if at_template_root && name_str == "settings.json" {
                let settings_dir = dest_dir.join(".agents").join("rhei");
                fs::create_dir_all(&settings_dir).map_err(|err| {
                    file_io_report(&settings_dir, "failed to create .agents/rhei directory", err)
                })?;
                settings_dir.join("settings.json")
            } else {
                dest_dir.join(&name)
            };
            let metadata = entry.metadata().map_err(|err| {
                file_io_report(&src_path, "failed to read template metadata", err)
            })?;

            if metadata.is_dir() {
                fs::create_dir_all(&dest_path).map_err(|err| {
                    file_io_report(&dest_path, "failed to create output directory", err)
                })?;
                fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                    file_io_report(&dest_path, "failed to preserve directory permissions", err)
                })?;
                materialize_template_dir(&src_path, &dest_path, template_root, values)?;
                continue;
            }

            if is_text_template_file(&src_path)? {
                let raw = fs::read_to_string(&src_path).map_err(|err| {
                    file_io_report(&src_path, "failed to read template text file", err)
                })?;
                let rendered = render_template_text(&raw, values, &src_path)?;
                // Template-shipped settings.json must parse as JSON after
                // instantiation-variable substitution. Catching this here
                // surfaces malformed bundles before `rhei validate` runs.
                if at_template_root && name_str == "settings.json" {
                    serde_json::from_str::<serde_json::Value>(&rendered).map_err(|err| {
                        miette!(
                            "template settings.json is not valid JSON after instantiation: {err}"
                        )
                    })?;
                }
                fs::write(&dest_path, rendered).map_err(|err| {
                    file_io_report(&dest_path, "failed to write output file", err)
                })?;
            } else {
                fs::copy(&src_path, &dest_path).map_err(|err| {
                    miette!(
                        "failed to copy '{}' to '{}': {err}",
                        src_path.display(),
                        dest_path.display()
                    )
                })?;
            }

            fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                file_io_report(&dest_path, "failed to preserve file permissions", err)
            })?;
        }

        Ok(())
    }

    fn is_text_template_file(path: &Path) -> MietteResult<bool> {
        let bytes = fs::read(path)
            .map_err(|err| file_io_report(path, "failed to read template file", err))?;
        Ok(!bytes[..bytes.len().min(8192)].contains(&0))
    }

    fn render_template_text(
        raw: &str,
        values: &BTreeMap<String, serde_json::Value>,
        path: &Path,
    ) -> MietteResult<String> {
        let literal_open = "__RHEI_TEMPLATE_LITERAL_OPEN__";
        let preprocessed = raw.replace(r"\{{", literal_open);
        let mut env = MiniJinjaEnvironment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        // MiniJinja strips a single trailing newline by default, which drops the
        // final newline from every instantiated file (states.yaml, settings.json,
        // task files, ...). Preserve it so rendered files keep the POSIX trailing
        // newline of their template source.
        env.set_keep_trailing_newline(true);
        env.add_filter("slug", |value: String| slugify_target_value(&value));

        let template = env
            .template_from_str(&preprocessed)
            .map_err(|err| miette!("failed to parse template '{}': {err}", path.display()))?;
        let rendered = template
            .render(values)
            .map_err(|err| miette!("failed to render template '{}': {err}", path.display()))?;
        Ok(rendered.replace(literal_open, "{{"))
    }
