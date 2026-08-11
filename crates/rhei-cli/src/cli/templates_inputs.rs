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
                        // §FS-rhei-errors.1.3
                        let names =
                            manifest.inputs.iter().map(|i| i.name.clone()).collect::<Vec<_>>();
                        return Err(miette!(
                            help = format!(
                                "{}List every input with: rhei instantiate {} --list-inputs",
                                did_you_mean(key, &names)
                                    .map(|hint| format!("{hint} "))
                                    .unwrap_or_default(),
                                manifest.name
                            ),
                            "template '{}' has no input named '{}'",
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
                        help = format!(
                            "this template takes {} positional value(s). Name the rest \
                             explicitly as KEY=VALUE — see: rhei instantiate {} --list-inputs",
                            positional_inputs.len(),
                            manifest.name
                        ),
                        "template '{}' has no positional slot {}",
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
            help = format!(
                "name each value: {}. List every input with: rhei instantiate {} --list-inputs",
                required
                    .iter()
                    .map(|input| format!("{}=<value>", input.name))
                    .collect::<Vec<_>>()
                    .join(" "),
                manifest.name
            ),
            "template '{}' does not accept positional inputs",
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
            .map_err(|err| {
                miette!(
                    help = "a --values file is a YAML or JSON object mapping input names to \
                            values. Fix the syntax at the position above.",
                    "failed to parse values file '{}': {err}",
                    path.display()
                )
            })?;
        let mapping = match value {
            YamlValue::Mapping(mapping) => mapping,
            _ => {
                return Err(miette!(
                    help = "write one `input_name: value` pair per line at the top level of \
                            the file.",
                    "values file '{}' must contain a YAML or JSON object at the top level",
                    path.display()
                ))
            }
        };

        let mut values = BTreeMap::new();
        for (key, value) in mapping {
            let Some(key) = key.as_str() else {
                return Err(miette!(
                    help = "every top-level key must be an input name, written as plain text.",
                    "values file '{}' contains a non-string key",
                    path.display()
                ));
            };
            values.insert(key.to_string(), value);
        }

        Ok(values)
    }

    fn parse_assignment(value: &str, flag_name: &str) -> MietteResult<(String, String)> {
        let Some((key, value)) = value.split_once('=') else {
            return Err(miette!(
                help = format!("write it as {} <input>=<value>", flag_name),
                "{} expects KEY=VALUE, got '{}'",
                flag_name,
                value
            ));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(miette!(
                help = format!("name the input before the '=': {} <input>=<value>", flag_name),
                "{} expects a non-empty key",
                flag_name
            ));
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
                _ => {
                    return Err(miette!(
                        help = format!("supply it as text, e.g. {label}='some text'"),
                        "{} for '{}' must be a string",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Number => match raw {
                YamlValue::Number(value) => serde_json::to_value(value)
                    .map_err(|err| miette!(
                        help = template_manifest_help(),
                        "failed to serialize number for '{}': {err}", label
                    ))?,
                YamlValue::String(value) => {
                    let trimmed = value.trim();
                    let number_re = Regex::new(r"^-?\d+(?:\.\d+)?$")
                        .expect("number validation regex should be valid");
                    if !number_re.is_match(trimmed) {
                        return Err(miette!(
                            help = format!("supply a plain number, e.g. {label}=3"),
                            "{} for '{}' must be a number, got '{}'",
                            source,
                            label,
                            trimmed
                        ));
                    }
                    let parsed: YamlValue = serde_yaml::from_str(trimmed).map_err(|err| {
                        miette!(
                            help = template_manifest_help(),
                            "{} for '{}' must be a number: {err}", source, label
                        )
                    })?;
                    serde_json::to_value(parsed).map_err(|err| {
                        miette!(
                            help = template_manifest_help(),
                            "failed to serialize number for '{}': {err}", label
                        )
                    })?
                }
                _ => {
                    return Err(miette!(
                        help = format!("supply a plain number, e.g. {label}=3"),
                        "{} for '{}' must be a number",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Boolean => match raw {
                YamlValue::Bool(value) => serde_json::Value::Bool(*value),
                YamlValue::String(value) => match value.trim() {
                    "true" => serde_json::Value::Bool(true),
                    "false" => serde_json::Value::Bool(false),
                    other => {
                        return Err(miette!(
                            help = format!("supply {label}=true or {label}=false"),
                            "{} for '{}' must be true or false, got '{}'",
                            source,
                            label,
                            other
                        ))
                    }
                },
                _ => {
                    return Err(miette!(
                        help = format!("supply {label}=true or {label}=false"),
                        "{} for '{}' must be true or false",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Path => match raw {
                YamlValue::String(value) => {
                    if value.is_empty() {
                        return Err(miette!(
                            help = format!("supply a path, e.g. {label}=./some/dir"),
                            "{} for '{}' must not be empty",
                            source,
                            label
                        ));
                    }
                    let path = PathBuf::from(value);
                    let resolved = if path.is_absolute() { path } else { cwd.join(path) };
                    if !from_default && !resolved.exists() {
                        // §FS-rhei-errors.6: a path input is resolved against the
                        // caller's cwd, so say which directory it was resolved from.
                        return Err(miette!(
                            help = format!(
                                "create it first, or point the input somewhere that exists. \
                                 Relative values resolve against {}",
                                cwd.display()
                            ),
                            "{} for '{}' refers to a path that does not exist: {}",
                            source,
                            label,
                            resolved.display()
                        ));
                    }
                    serde_json::Value::String(resolved.display().to_string())
                }
                _ => {
                    return Err(miette!(
                        help = format!("supply a path, e.g. {label}=./some/dir"),
                        "{} for '{}' must be a path string",
                        source,
                        label
                    ))
                }
            },
            TemplateInputType::Array => {
                let sequence = parse_template_sequence(label, raw, source)?;
                let item_schema = schema.items.as_deref().ok_or_else(|| {
                    miette!(
                        help = internal_error_help(),
                        "{} for '{}' requires an items schema",
                        source,
                        label
                    )
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
                            help = format!(
                                "write {label} as an object whose keys are property names, \
                                 e.g. {label}='{{key: value}}'"
                            ),
                            "{} for '{}' contains a non-string key",
                            source,
                            label
                        ));
                    };
                    let property_schema = schema.properties.get(key).ok_or_else(|| {
                        let known = schema.properties.keys().cloned().collect::<Vec<_>>();
                        miette!(
                            help = did_you_mean(key, &known)
                                .unwrap_or_else(|| format!("'{label}' declares no properties")),
                            "{} for '{}' contains unknown property '{}'",
                            source,
                            label,
                            key
                        )
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
                            help = format!(
                                "add it to the value, e.g. {label}='{{{property}: <value>}}'"
                            ),
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
            _ => Err(miette!(
                help = format!("write it as a YAML or JSON list, e.g. {label}='[one, two]'"),
                "{} for '{}' must be an array",
                source,
                label
            )),
        }
    }

    fn parse_template_mapping(
        label: &str,
        raw: &YamlValue,
        source: &str,
    ) -> MietteResult<YamlMapping> {
        match parse_structured_template_value(raw, "object", label, source)? {
            YamlValue::Mapping(values) => Ok(values),
            _ => Err(miette!(
                help = format!("write it as a YAML or JSON object, e.g. {label}='{{key: value}}'"),
                "{} for '{}' must be an object",
                source,
                label
            )),
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
                    help = format!(
                        "{label} takes a YAML or JSON {expected} on the command line, e.g. \
                         {}",
                        if expected == "array" {
                            format!("{label}='[one, two]'")
                        } else {
                            format!("{label}='{{key: value}}'")
                        }
                    ),
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

    /// A default rendered the way the user would supply it on a command line:
    /// one line, flow style, shell-quoted. §FS-rhei-errors.2
    fn compact_default_assignment(input: &TemplateInputDef) -> Option<String> {
        let default = input.schema.default.as_ref()?;
        // `serde_json` emits flow style for both sequences and mappings, which
        // is the syntax `rhei instantiate` parses a value back out of.
        let compact = serde_json::to_string(default).ok()?;
        Some(shell_assignment(&input.name, &compact))
    }

    fn print_template_inputs(manifest: &TemplateManifest, template_ref: &str) {
        println!("Template: {}", manifest.name);
        println!("Version: {}", manifest.version_string());
        println!("Description: {}", manifest.description);

        if manifest.inputs.is_empty() {
            println!("Inputs: none");
            println!();
            println!("Instantiate it with:");
            println!("  {}", shell_command(["rhei", "instantiate", template_ref]));
            return;
        }

        println!("Inputs:");
        for input in &manifest.inputs {
            // A structured default renders as multi-line YAML, which a
            // single-line `(type, default=…)` parenthetical simply tore apart.
            // Scalars stay inline; anything taller gets its own block.
            let rendered_default = input.schema.default.as_ref().map(format_version);
            let block_default =
                rendered_default.as_deref().filter(|rendered| rendered.contains('\n'));
            let requirement = if input.is_required() {
                "required".to_string()
            } else if block_default.is_some() {
                "default below".to_string()
            } else if let Some(default) = rendered_default.as_deref() {
                // §FS-rhei-errors.2: this listing is where users copy values
                // from, and a `[mode]` selector is a glob in zsh unquoted.
                format!("default={}", shell_quote(default))
            } else {
                "optional".to_string()
            };
            println!("  {} ({}, {})", input.name, input.value_type().as_str(), requirement);
            println!("    {}", input.description);
            if let Some(default) = block_default {
                println!("    default:");
                for line in default.lines() {
                    println!("      {line}");
                }
                // §FS-rhei-errors.2: the block above is readable but its
                // scalars are bare YAML, so follow it with a pasteable form.
                if let Some(compact) = compact_default_assignment(input) {
                    println!("    copy: {compact}");
                }
            }
            if let Some(pattern) = input.schema.validate.as_deref() {
                println!("    validate: {}", pattern);
            }
            if let Some(format) = input.schema.format {
                println!("    format: {}", format.as_str());
            }
        }

        // End on the command, not on the inventory.
        // §FS-rhei-errors.1.2 §FS-rhei-templates.6.3
        let required = manifest
            .inputs
            .iter()
            .filter(|input| input.is_required() && input.schema.default.is_none())
            .map(|input| format!("{}={}", input.name, template_input_placeholder(input)))
            .collect::<Vec<_>>();
        println!();
        println!("Instantiate it with:");
        println!(
            "  {}",
            format_template_instantiation_command(template_ref, &[], &[], &[], &[], None, &required)
        );
    }

    /// Where a materialization writes, and how a destination inside it may be
    /// named in an error. A `--dry-run` temp directory is removed before the
    /// user could look at it, so its paths must not appear. §FS-rhei-errors.4
    struct MaterializeTarget<'a> {
        root: &'a Path,
        /// True when `root` is scratch space the user neither chose nor keeps.
        scratch: bool,
    }

    impl MaterializeTarget<'_> {
        /// A diagnostic for a filesystem failure at `dest`.
        fn io_report(&self, dest: &Path, action: &str, err: std::io::Error) -> Report {
            if !self.scratch {
                return file_io_report(dest, action, err);
            }
            // Name the file by its place in the template, which the user can
            // open, rather than by a temp path that no longer exists.
            let relative = dest.strip_prefix(self.root).unwrap_or(dest);
            miette!(
                help = "--dry-run renders into a temp directory. Check that $TMPDIR exists, \
                        is writable, and has free space.",
                "{action} '{}' while rendering the template: {err}",
                relative.display()
            )
        }
    }

    fn materialize_template(
        template_dir: &Path,
        layout: TemplateLayout,
        output_dir: &Path,
        values: &BTreeMap<String, serde_json::Value>,
        scratch: bool,
    ) -> MietteResult<MaterializedTemplate> {
        let target = MaterializeTarget { root: output_dir, scratch };
        fs::create_dir_all(output_dir)
            .map_err(|err| target.io_report(output_dir, "failed to create output directory", err))?;
        let root_permissions = fs::metadata(template_dir)
            .map_err(|err| file_io_report(template_dir, "failed to read template metadata", err))?
            .permissions();
        fs::set_permissions(output_dir, root_permissions).map_err(|err| {
            target.io_report(output_dir, "failed to preserve output directory permissions", err)
        })?;

        materialize_template_dir(template_dir, output_dir, template_dir, values, &target)?;

        Ok(MaterializedTemplate { layout, output_dir: output_dir.to_path_buf() })
    }

    fn materialize_template_dir(
        src_dir: &Path,
        dest_dir: &Path,
        template_root: &Path,
        values: &BTreeMap<String, serde_json::Value>,
        target: &MaterializeTarget<'_>,
    ) -> MietteResult<()> {
        let mut entries = fs::read_dir(src_dir)
            .map_err(|err| file_io_report(src_dir, "failed to read template directory", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!(
                help = "check that the template directory is readable.",
                "failed to read dir entry in '{}': {err}", src_dir.display()
            ))?;
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
                    target.io_report(&settings_dir, "failed to create .agents/rhei directory", err)
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
                    target.io_report(&dest_path, "failed to create output directory", err)
                })?;
                fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                    target.io_report(&dest_path, "failed to preserve directory permissions", err)
                })?;
                materialize_template_dir(&src_path, &dest_path, template_root, values, target)?;
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
                            help = "the template's settings.json is malformed once inputs are substituted. Fix the template file, then re-run.",
                            "template settings.json is not valid JSON after instantiation: {err}"
                        )
                    })?;
                }
                fs::write(&dest_path, rendered).map_err(|err| {
                    target.io_report(&dest_path, "failed to write output file", err)
                })?;
            } else {
                fs::copy(&src_path, &dest_path)
                    .map_err(|err| target.io_report(&dest_path, "failed to copy into", err))?;
            }

            fs::set_permissions(&dest_path, metadata.permissions()).map_err(|err| {
                target.io_report(&dest_path, "failed to preserve file permissions", err)
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
            .map_err(|err| miette!(
                help = "this template's text contains an invalid {{ }} expression. Fix the template file, then re-run.",
                "failed to parse template '{}': {err}", path.display()
            ))?;
        let rendered = template
            .render(values)
            .map_err(|err| miette!(
                help = "this template references an input it does not declare, or applies a filter to the wrong type. Fix the template file, then re-run.",
                "failed to render template '{}': {err}", path.display()
            ))?;
        Ok(rendered.replace(literal_open, "{{"))
    }
