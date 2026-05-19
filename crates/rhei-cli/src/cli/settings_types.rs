
/// Rhei settings loaded from `~/.config/rhei/settings.json` or `.rhei/settings.json`.
#[derive(Debug, Default, Deserialize, Clone)]
struct RheiSettings {
    #[serde(default)]
    agent: Option<AgentConfig>,
    #[serde(default)]
    agent_mode: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    agent_timeout: Option<String>,
    #[serde(default)]
    program_timeout: Option<String>,
    /// Spec-aligned nested defaults. The `defaults.{model, agent,
    /// agent_mode, agent_timeout, program_timeout, mcp_servers, skills}` keys
    /// are the canonical settings shape from
    /// `docs/functional-spec/rhei-agents.spec.md` §Agent Configuration. The
    /// top-level `agent` / `model` / `agent_timeout` / `program_timeout` /
    /// `agent_mode` fields above remain readable for backward compatibility.
    #[serde(default)]
    defaults: SettingsDefaults,
    /// Registry of agent transport profiles keyed by agent id.
    #[serde(default)]
    agents: BTreeMap<String, CustomAgentProfile>,
    /// Registry of model profiles keyed by model id. See
    /// `docs/functional-spec/rhei-agents.spec.md` §`models`.
    #[serde(default)]
    models: BTreeMap<String, ModelProfile>,
    /// Registry of MCP server profiles keyed by server id.
    #[serde(default)]
    mcp_servers: BTreeMap<String, McpServerProfile>,
    /// Registry of skill profiles keyed by skill id.
    #[serde(default)]
    skills: BTreeMap<String, SkillProfile>,
    /// Top-level snapshots block. The field is retained verbatim from
    /// settings so the snapshot subsystem (impl-rhei-snapshots) can read its
    /// configured `cache_dir`, `redactor`, and adapter gates without
    /// reparsing the file. See
    /// `docs/functional-spec/rhei-agents.spec.md` §`snapshots` and
    /// `docs/functional-spec/rhei-snapshot-operations.spec.md` §Configuration
    /// for the authoritative schema.
    #[serde(default)]
    snapshots: Option<SnapshotSettings>,
}

/// Top-level `snapshots` settings block.
///
/// `cache_dir` defaults to `.rhei/cache/snapshots` under the plan workspace;
/// fields omitted here inherit from global settings before defaults are
/// applied. See
/// `docs/functional-spec/rhei-snapshot-operations.spec.md` §4.1 Settings
/// Block and §4.2 Privacy: Redaction Hook.
#[derive(Debug, Default, Deserialize, Clone)]
struct SnapshotSettings {
    #[serde(default)]
    cache_dir: Option<PathBuf>,
    #[serde(default)]
    experimental: Option<serde_json::Value>,
    #[serde(default)]
    provider_cache_ttl: BTreeMap<String, String>,
    #[serde(default)]
    redactor: Option<PathBuf>,
    /// Optional allow-list for redactor environment forwarding. The v1 hook
    /// keeps the parent environment closed by default per §4.2.
    #[serde(default)]
    redactor_env: Vec<String>,
}

fn merge_snapshot_settings(
    global: Option<SnapshotSettings>,
    project: Option<SnapshotSettings>,
) -> Option<SnapshotSettings> {
    match (global, project) {
        (None, None) => None,
        (Some(settings), None) | (None, Some(settings)) => Some(settings),
        (Some(mut global), Some(project)) => {
            if project.cache_dir.is_some() {
                global.cache_dir = project.cache_dir;
            }
            if project.experimental.is_some() {
                global.experimental = project.experimental;
            }
            for (provider, ttl) in project.provider_cache_ttl {
                global.provider_cache_ttl.insert(provider, ttl);
            }
            if project.redactor.is_some() {
                global.redactor = project.redactor;
            }
            if !project.redactor_env.is_empty() {
                global.redactor_env = project.redactor_env;
            }
            Some(global)
        }
    }
}

fn snapshot_cache_dir(settings: &RheiSettings, workspace_root: &Path) -> PathBuf {
    let configured = settings
        .snapshots
        .as_ref()
        .and_then(|snapshots| snapshots.cache_dir.clone())
        .unwrap_or_else(|| PathBuf::from(".rhei/cache/snapshots"));
    if configured.is_absolute() {
        configured
    } else {
        workspace_root.join(configured)
    }
}

/// One entry in the merged `models` registry. See
/// `docs/functional-spec/rhei-agents.spec.md` §`models`.
#[derive(Debug, Default, Deserialize, Clone)]
struct ModelProfile {
    /// Provider identifier such as `anthropic` or `openai`.
    #[serde(default)]
    provider: Option<String>,
    /// Concrete provider model name (`claude-sonnet-4-6`, `o3`, ...). Passed
    /// to the agent's `model_flag` when present.
    #[serde(default)]
    model: Option<String>,
    /// Preferred agent id when `rhei run` needs to spawn this model
    /// autonomously and no other level configured one.
    #[serde(default)]
    default_agent: Option<String>,
    /// Per-agent launch overrides for this model, keyed by agent id.
    #[serde(default)]
    agents: BTreeMap<String, ModelAgentBinding>,
}

/// One `models.<id>.agents.<agent>` binding. Only `timeout` is consumed by
/// `rhei run` today; `args` and `autonomous_args` are accepted by the parser
/// for forward compatibility.
#[derive(Debug, Default, Deserialize, Clone)]
struct ModelAgentBinding {
    #[serde(default)]
    #[allow(dead_code)]
    args: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    autonomous_args: Vec<String>,
    #[serde(default)]
    timeout: Option<String>,
}

/// Nested `defaults` section in settings.
///
/// `mcp_servers` and `skills` use `Option<Vec<_>>` so the merge layer can
/// distinguish "unset" (inherit) from "empty" (explicitly clear inherited).
#[derive(Debug, Default, Deserialize, Clone)]
struct SettingsDefaults {
    /// Default model profile id. See spec §`defaults`.
    #[serde(default)]
    model: Option<String>,
    /// Default agent id resolved against the `agents` registry. The spec
    /// requires a bare string id — inline agent objects are rejected by
    /// `AgentConfig`'s transparent deserialisation, which surfaces a JSON
    /// type error.
    #[serde(default)]
    agent: Option<AgentConfig>,
    /// Default agent mode applied when a state does not set `agent_mode`.
    /// `null` explicitly clears an inherited default.
    #[serde(default)]
    agent_mode: Option<String>,
    #[serde(default)]
    agent_timeout: Option<String>,
    /// Default program timeout. See spec §`defaults`.
    #[serde(default)]
    program_timeout: Option<String>,
    #[serde(default)]
    mcp_servers: Option<Vec<StateMcpEntry>>,
    #[serde(default)]
    skills: Option<Vec<StateSkillEntry>>,
}

/// Built-in agent registry.
///
/// Each entry is a ready-to-use `CustomAgentProfile` for one of the agents
/// that Rhei supports out of the box. The per-agent "autonomous" flag set
/// that was hard-coded as `default_args` is now exposed as a named `yolo`
/// mode so states and defaults can select it explicitly via `agent_mode`.
///
/// A user-written entry with the same id in global or project settings
/// replaces the built-in entry wholesale (see `load_merged_settings`).
fn built_in_agents() -> BTreeMap<String, CustomAgentProfile> {
    fn flags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    let modes_yolo_only = |yolo: Vec<String>| {
        let mut modes = IndexMap::new();
        modes.insert("yolo".to_string(), yolo);
        modes
    };

    let mut agents = BTreeMap::new();

    agents.insert(
        "claude-code".to_string(),
        CustomAgentProfile {
            command: flags(&["claude"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            mcp_config_flag: Some("--mcp-config".to_string()),
            skill_flag: Some("--skill".to_string()),
            modes: modes_yolo_only(flags(&["--permission-mode", "bypassPermissions"])),
            ..Default::default()
        },
    );

    // codex: `codex exec` is non-interactive. The `yolo` mode mirrors the
    // spec table at docs/functional-spec/rhei-agents.spec.md §Known Agent
    // Profiles — `--sandbox danger-full-access --skip-git-repo-check
    // -c approval_policy="never"`. `-c approval_policy="never"` replaced the
    // older `-a never` short flag, which codex-cli no longer accepts.
    agents.insert(
        "codex".to_string(),
        CustomAgentProfile {
            command: flags(&["codex", "exec"]),
            prompt_flag: None,
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            mcp_flag: Some("--mcp".to_string()),
            modes: modes_yolo_only(flags(&[
                "--sandbox",
                "danger-full-access",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"never\"",
            ])),
            ..Default::default()
        },
    );

    // gemini: `--approval-mode yolo` is the autonomous posture
    // (`auto_edit` still prompts on shell tool calls).
    agents.insert(
        "gemini".to_string(),
        CustomAgentProfile {
            command: flags(&["gemini"]),
            prompt_flag: Some("--prompt".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--approval-mode", "yolo"])),
            ..Default::default()
        },
    );

    // kilocode: `kilo --auto "<prompt>"` is the documented CI invocation;
    // `--yolo` auto-approves tool permissions. `--auto` takes the prompt
    // as its argument, so it maps onto `prompt_flag`.
    agents.insert(
        "kilocode".to_string(),
        CustomAgentProfile {
            command: flags(&["kilo"]),
            prompt_flag: Some("--auto".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--yolo"])),
            ..Default::default()
        },
    );

    // cursor: the headless binary is `cursor-agent` (distinct from the
    // `cursor` IDE launcher). `-p`/`--print` is the non-interactive flag;
    // `--force` is the auto-approve posture.
    agents.insert(
        "cursor".to_string(),
        CustomAgentProfile {
            command: flags(&["cursor-agent"]),
            prompt_flag: Some("--print".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--force"])),
            ..Default::default()
        },
    );

    // pi (openclaw / badlogic pi-coding-agent). Headless mode exits
    // deterministically after one turn. pi has no permission layer — modes
    // are intentionally empty; isolation is the caller's responsibility
    // (e.g. sandbox/container). See docs/functional-spec/rhei-agents.spec.md §Known
    // Agent Profiles.
    agents.insert(
        "pi".to_string(),
        CustomAgentProfile {
            command: flags(&["pi"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            skill_flag: Some("--skill".to_string()),
            session: Some(serde_json::json!({
                "resume": {"flag": "--continue"},
                "fork": {"flag": "--fork"},
                "session_dir_flag": "--session-dir",
                "no_session_flag": "--no-session",
                "layout": {"kind": "FlatById", "ext": "jsonl"}
            })),
            ..Default::default()
        },
    );

    agents
}

#[derive(Debug, Clone)]
struct SettingsDocument {
    raw: serde_json::Value,
    typed: RheiSettings,
}

fn empty_settings_document() -> SettingsDocument {
    SettingsDocument {
        raw: serde_json::Value::Object(serde_json::Map::new()),
        typed: RheiSettings::default(),
    }
}

fn load_settings_document(path: &Path) -> MietteResult<SettingsDocument> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_settings_document());
        }
        Err(err) => {
            return Err(miette!("failed to read settings '{}': {err}", path.display()));
        }
    };
    let raw: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|err| miette!("failed to parse settings '{}': {err}", path.display()))?;
    let typed: RheiSettings = serde_json::from_value(raw.clone())
        .map_err(|err| miette!("failed to decode settings '{}': {err}", path.display()))?;
    Ok(SettingsDocument { raw, typed })
}
