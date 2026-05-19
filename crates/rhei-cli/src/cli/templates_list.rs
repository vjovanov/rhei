    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateSource {
        Project,
        User,
    }

    impl TemplateSource {
        fn as_str(self) -> &'static str {
            match self {
                TemplateSource::Project => "project",
                TemplateSource::User => "user",
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateSourceFilter {
        Project,
        User,
        All,
    }

    impl TemplateSourceFilter {
        fn includes(self, source: TemplateSource) -> bool {
            matches!(
                (self, source),
                (TemplateSourceFilter::All, _)
                    | (TemplateSourceFilter::Project, TemplateSource::Project)
                    | (TemplateSourceFilter::User, TemplateSource::User)
            )
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TemplateLayout {
        SingleFile,
        Workspace,
    }

    impl TemplateLayout {
        fn entrypoint(self, output_dir: &Path) -> PathBuf {
            match self {
                TemplateLayout::SingleFile => output_dir.join("plan.rhei.md"),
                TemplateLayout::Workspace => output_dir.to_path_buf(),
            }
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TemplateManifest {
        name: String,
        version: YamlValue,
        description: String,
        #[serde(default)]
        inputs: Vec<TemplateInputDef>,
    }

    impl TemplateManifest {
        fn version_string(&self) -> String {
            format_version(&self.version)
        }

        fn required_input_count(&self) -> usize {
            self.inputs.iter().filter(|input| input.is_required()).count()
        }

        fn inputs_summary(&self) -> String {
            if self.inputs.is_empty() {
                return "none".to_string();
            }

            self.inputs
                .iter()
                .map(|input| {
                    if input.is_required() {
                        input.name.clone()
                    } else {
                        format!("{}?", input.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TemplateInputDef {
        name: String,
        description: String,
        #[serde(default)]
        positional: Option<usize>,
        #[serde(flatten)]
        schema: TemplateValueSchema,
    }

    impl TemplateInputDef {
        fn is_required(&self) -> bool {
            self.schema.is_required()
        }

        fn value_type(&self) -> TemplateInputType {
            self.schema.value_type
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TemplateValueSchema {
        #[serde(default, rename = "type")]
        value_type: TemplateInputType,
        #[serde(default)]
        required: Option<bool>,
        #[serde(default)]
        default: Option<YamlValue>,
        #[serde(default)]
        validate: Option<String>,
        #[serde(default)]
        items: Option<Box<TemplateValueSchema>>,
        #[serde(default)]
        properties: BTreeMap<String, TemplateValueSchema>,
    }

    impl TemplateValueSchema {
        fn is_required(&self) -> bool {
            self.required.unwrap_or(self.default.is_none())
        }
    }

    #[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum TemplateInputType {
        #[default]
        String,
        Number,
        Boolean,
        Path,
        Array,
        Object,
    }

    impl TemplateInputType {
        fn as_str(self) -> &'static str {
            match self {
                TemplateInputType::String => "string",
                TemplateInputType::Number => "number",
                TemplateInputType::Boolean => "boolean",
                TemplateInputType::Path => "path",
                TemplateInputType::Array => "array",
                TemplateInputType::Object => "object",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct DiscoveredTemplate {
        manifest: TemplateManifest,
        path: PathBuf,
        source: TemplateSource,
    }

    #[derive(Debug)]
    struct MaterializedTemplate {
        layout: TemplateLayout,
        output_dir: PathBuf,
    }

    impl MaterializedTemplate {
        fn entrypoint(&self) -> PathBuf {
            self.layout.entrypoint(&self.output_dir)
        }

        fn state_machine_path(&self) -> Option<PathBuf> {
            let path = self.output_dir.join("states.yaml");
            path.is_file().then_some(path)
        }
    }

    pub(super) fn templates_command(as_json: bool, source_filter: &str) -> MietteResult<()> {
        let filter = parse_template_source_filter(source_filter)?;
        let templates = discover_templates(filter)?;

        if as_json {
            let payload = templates
                .iter()
                .map(|template| {
                    serde_json::json!({
                        "name": template.manifest.name,
                        "version": template.manifest.version_string(),
                        "description": template.manifest.description,
                        "source": template.source.as_str(),
                        "path": template.path,
                        "required_inputs": template.manifest.required_input_count(),
                        "inputs": template.manifest.inputs.iter().map(|input| {
                            serde_json::json!({
                                "name": input.name,
                                "type": input.value_type().as_str(),
                                "required": input.is_required(),
                                "description": input.description,
                                "default": input.schema.default.as_ref(),
                                "validate": input.schema.validate.as_ref(),
                            })
                        }).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            let rendered = serde_json::to_string_pretty(&payload)
                .map_err(|err| miette!("failed to serialize template listing: {err}"))?;
            println!("{rendered}");
            return Ok(());
        }

        if templates.is_empty() {
            println!("No templates found.");
            let roots = template_search_roots(filter)?;
            if !roots.is_empty() {
                println!("Searched:");
                for (source, root) in roots {
                    let exists_marker = if root.is_dir() { "" } else { " (does not exist)" };
                    println!("  [{}] {}{}", source.as_str(), root.display(), exists_marker);
                }
            }
            return Ok(());
        }

        println!("Templates:");
        for template in templates {
            println!(
                "{}  {}  {}",
                template.manifest.name,
                template.manifest.version_string(),
                template.source.as_str(),
            );
            println!("  {}", template.manifest.description);
            println!("  inputs: {}", template.manifest.inputs_summary());
        }

        Ok(())
    }

    pub(super) fn complete_template_reference(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some(current_str) = current.to_str() else {
            return PathCompleter::dir().complete(current);
        };

        if template_reference_is_path(current_str) {
            return PathCompleter::dir().complete(current);
        }

        let Ok(templates) = discover_templates(TemplateSourceFilter::All) else {
            return Vec::new();
        };

        templates
            .into_iter()
            .filter(|template| template.manifest.name.starts_with(current_str))
            .map(|template| {
                let help =
                    format!("{} ({})", template.manifest.description, template.source.as_str());
                CompletionCandidate::new(template.manifest.name).help(Some(help.into()))
            })
            .collect()
    }

    pub(super) fn complete_template_input_arg(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some((manifest, input_args)) = completion_template_context() else {
            return Vec::new();
        };
        complete_template_input_value(&manifest, &input_args, current, false)
    }

    pub(super) fn complete_template_set_value(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some((manifest, input_args)) = completion_template_context() else {
            return Vec::new();
        };
        complete_template_assignment(&manifest, &input_args, current, false)
    }

    pub(super) fn complete_template_set_file(current: &OsStr) -> Vec<CompletionCandidate> {
        let Some((manifest, input_args)) = completion_template_context() else {
            return Vec::new();
        };
        complete_template_assignment(&manifest, &input_args, current, true)
    }

    fn complete_template_input_value(
        manifest: &TemplateManifest,
        prior_input_args: &[String],
        current: &OsStr,
        set_file: bool,
    ) -> Vec<CompletionCandidate> {
        let current_str = current.to_string_lossy();
        if current_str.contains('=') {
            return complete_template_assignment(manifest, prior_input_args, current, set_file);
        }

        let mut candidates = Vec::new();
        if let Some(input) = next_positional_input(manifest, prior_input_args) {
            candidates.extend(complete_template_value_for_input(input, current, None, false));
        }
        candidates.extend(complete_template_assignment_keys(manifest, prior_input_args, current));
        candidates
    }

    fn complete_template_assignment(
        manifest: &TemplateManifest,
        prior_input_args: &[String],
        current: &OsStr,
        set_file: bool,
    ) -> Vec<CompletionCandidate> {
        let current_str = current.to_string_lossy();
        let Some((key, value_prefix)) = current_str.split_once('=') else {
            return complete_template_assignment_keys(manifest, prior_input_args, current);
        };

        let Some(input) = manifest.inputs.iter().find(|input| input.name == key) else {
            return Vec::new();
        };
        complete_template_value_for_input(input, OsStr::new(value_prefix), Some(key), set_file)
    }

    fn complete_template_assignment_keys(
        manifest: &TemplateManifest,
        prior_input_args: &[String],
        current: &OsStr,
    ) -> Vec<CompletionCandidate> {
        let prefix = current.to_string_lossy();
        let supplied = supplied_template_input_keys(manifest, prior_input_args);
        manifest
            .inputs
            .iter()
            .filter(|input| !supplied.contains(input.name.as_str()))
            .filter(|input| input.name.starts_with(prefix.as_ref()))
            .map(|input| {
                CompletionCandidate::new(format!("{}=", input.name))
                    .help(Some(template_input_help(input).into()))
            })
            .collect()
    }

    fn complete_template_value_for_input(
        input: &TemplateInputDef,
        current: &OsStr,
        assignment_key: Option<&str>,
        set_file: bool,
    ) -> Vec<CompletionCandidate> {
        let mut candidates = if set_file {
            PathCompleter::file().complete(current)
        } else {
            match input.value_type() {
                TemplateInputType::Path => PathCompleter::any().complete(current),
                TemplateInputType::Boolean => static_completion(
                    current,
                    &[("true", "Boolean true"), ("false", "Boolean false")],
                ),
                TemplateInputType::Array => static_completion(
                    current,
                    &[("[]", "Empty array"), ("[item]", "Array snippet")],
                ),
                TemplateInputType::Object => static_completion(current, &[("{}", "Empty object")]),
                TemplateInputType::String | TemplateInputType::Number => Vec::new(),
            }
        };

        if let Some(key) = assignment_key {
            let prefix = format!("{key}=");
            candidates =
                candidates.into_iter().map(|candidate| candidate.add_prefix(&prefix)).collect();
        }
        candidates
    }

    fn template_input_help(input: &TemplateInputDef) -> String {
        let requirement = if let Some(default) = input.schema.default.as_ref() {
            format!("default {}", format_version(default))
        } else if input.is_required() {
            "required".to_string()
        } else {
            "optional".to_string()
        };
        let positional =
            input.positional.map(|index| format!(", positional {index}")).unwrap_or_default();
        format!(
            "{}, {}{} - {}",
            input.value_type().as_str(),
            requirement,
            positional,
            input.description
        )
    }

    fn next_positional_input<'a>(
        manifest: &'a TemplateManifest,
        prior_input_args: &[String],
    ) -> Option<&'a TemplateInputDef> {
        let positional_count = prior_input_args
            .iter()
            .filter(|value| !template_input_arg_is_assignment(manifest, value))
            .count();
        let next_position = positional_count + 1;
        if let Some(input) =
            manifest.inputs.iter().find(|input| input.positional == Some(next_position))
        {
            return Some(input);
        }
        if manifest.inputs.iter().all(|input| input.positional.is_none()) && positional_count == 0 {
            let required =
                manifest.inputs.iter().filter(|input| input.is_required()).collect::<Vec<_>>();
            if required.len() == 1 {
                return Some(required[0]);
            }
        }
        None
    }

    fn supplied_template_input_keys<'a>(
        manifest: &'a TemplateManifest,
        prior_input_args: &'a [String],
    ) -> HashSet<&'a str> {
        let mut supplied = HashSet::new();
        let mut positional_index = 1;
        for value in prior_input_args {
            if let Some((key, _)) = value.split_once('=') {
                if manifest.inputs.iter().any(|input| input.name == key) {
                    supplied.insert(key);
                    continue;
                }
            }
            if let Some(input) =
                manifest.inputs.iter().find(|input| input.positional == Some(positional_index))
            {
                supplied.insert(input.name.as_str());
                positional_index += 1;
            } else if manifest.inputs.iter().all(|input| input.positional.is_none()) {
                let required =
                    manifest.inputs.iter().filter(|input| input.is_required()).collect::<Vec<_>>();
                if required.len() == 1 && positional_index == 1 {
                    supplied.insert(required[0].name.as_str());
                    positional_index += 1;
                }
            }
        }
        supplied
    }

    fn template_input_arg_is_assignment(manifest: &TemplateManifest, value: &str) -> bool {
        value
            .split_once('=')
            .is_some_and(|(key, _)| manifest.inputs.iter().any(|input| input.name == key))
    }

    fn completion_template_context() -> Option<(TemplateManifest, Vec<String>)> {
        let words = completion_words();
        let instantiate_index = words.iter().position(|word| word == "instantiate")?;
        let words = words.get(instantiate_index + 1..)?;
        let before_current = words.get(..words.len().saturating_sub(1)).unwrap_or(words);
        let (template, input_args) = completion_template_and_inputs(before_current)?;
        let template_dir = resolve_template_reference(&template).ok()?;
        let manifest = load_template_manifest(&template_dir).ok()?;
        Some((manifest, input_args))
    }

    fn completion_template_and_inputs(words: &[String]) -> Option<(String, Vec<String>)> {
        let mut template = None;
        let mut input_args = Vec::new();
        let mut expects_value_for: Option<&str> = None;

        for word in words {
            if word.is_empty() {
                break;
            }
            if let Some(option) = expects_value_for.take() {
                if matches!(option, "set" | "set-file") {
                    input_args.push(word.clone());
                }
                continue;
            }
            if let Some(option) = word.strip_prefix("--") {
                if let Some((name, value)) = option.split_once('=') {
                    if matches!(name, "set" | "set-file") {
                        input_args.push(value.to_string());
                    }
                    continue;
                }
                if matches!(option, "set" | "set-file" | "values" | "output") {
                    expects_value_for = Some(option);
                }
                continue;
            }
            if template.is_none() {
                template = Some(word.clone());
            } else {
                input_args.push(word.clone());
            }
        }

        template.map(|template| (template, input_args))
    }
