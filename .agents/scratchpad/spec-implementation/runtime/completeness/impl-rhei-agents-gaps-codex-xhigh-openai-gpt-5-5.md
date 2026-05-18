# Completeness Audit: impl-rhei-agents

Spec: `docs/functional-spec/rhei-agents.spec.md` (from `runtime/manifests/impl-rhei-agents-spec.txt`)
Repo root: `/home/vjovanov/c/rhei`

Status vocabulary: `covered`, `partial`, `missing`, `not-normative`.

## Inventory

### Overview

1. `not-normative` — Spec scope statement: the document covers agent configuration, resolution, invocation, prompt composition, parallel execution, timeout handling, and log capture. This is descriptive scope text, not an implementation requirement. Evidence: `docs/functional-spec/rhei-agents.spec.md:3`.

2. `partial` — `rhei run` can spawn coding agents directly, resolve an agent for each task, compose a prompt, spawn a subprocess, and remain transition authority after subprocess exit. Agent spawning, prompt composition, and post-exit auto-transition are implemented, but completion is not strictly "for each task" under the `rhei next` claimability rule because `run` uses `find_ready_tasks`, not `find_claimable_tasks`, and does not check assignees. Evidence: `docs/functional-spec/rhei-agents.spec.md:9`, `crates/rhei-cli/src/main.rs:7808`, `crates/rhei-cli/src/main.rs:9123`, `crates/rhei-cli/src/main.rs:9168`, `crates/rhei-cli/src/main.rs:8311`, `crates/rhei-cli/src/main.rs:8343`, `crates/rhei-cli/src/main.rs:8439`.

3. `covered` — `rhei run` is transition authority after the subprocess exits; the agent subprocess itself is not used as transition authority in the implemented run loop. Evidence: `docs/functional-spec/rhei-agents.spec.md:9`, `crates/rhei-cli/src/main.rs:8386`, `crates/rhei-cli/src/main.rs:8439`, `crates/rhei-cli/src/main.rs:8672`.

4. `covered` — Callbacks still fire as part of engine-driven transitions unless `--no-callbacks` is set. Evidence: `docs/functional-spec/rhei-agents.spec.md:9`, `crates/rhei-cli/src/main.rs:8439`, `crates/rhei-cli/src/main.rs:5033`, `crates/rhei-cli/src/main.rs:5215`, `crates/rhei-cli/src/main.rs:5391`.

### Agent Configuration

5. `not-normative` — The model/agent/tooling separation explanation defines concepts and motivation. Evidence: `docs/functional-spec/rhei-agents.spec.md:13`.

6. `partial` — Global and project settings paths are supported, but project settings are only loaded from `<plan-root>/.rhei/settings.json`; the spec also says project settings may be in the workspace or plan directory. For single-file plans, `workspace_root` is the plan parent, so this is covered for the plan directory case. Evidence: `docs/functional-spec/rhei-agents.spec.md:31`, `crates/rhei-cli/src/main.rs:5922`, `crates/rhei-cli/src/main.rs:5928`, `crates/rhei-cli/src/main.rs:7688`.

7. `partial` — Both settings files use the same schema and compose by key. The implementation composes several top-level registries, but the schema is not fully aligned: nested `defaults.model` and `defaults.agent` are absent, and settings parse failures silently become defaults. Evidence: `docs/functional-spec/rhei-agents.spec.md:38`, `crates/rhei-cli/src/main.rs:5913`, `crates/rhei-cli/src/main.rs:5916`, `crates/rhei-cli/src/main.rs:5723`, `crates/rhei-cli/src/main.rs:5779`.

8. `partial` — `defaults.model` is a supported default model profile id. Implementation supports legacy top-level `model`, but `SettingsDefaults` has no `model` field, so spec-shaped `defaults.model` is ignored. Evidence: `docs/functional-spec/rhei-agents.spec.md:141`, `crates/rhei-cli/src/main.rs:5715`, `crates/rhei-cli/src/main.rs:5779`, `crates/rhei-cli/src/main.rs:5966`, `crates/rhei-cli/src/main.rs:6378`.

9. `partial` — `defaults.agent` is a supported default agent id and inline objects are not accepted. Implementation supports legacy top-level `agent`, but `SettingsDefaults` has no `agent` field, so spec-shaped `defaults.agent` is ignored. Evidence: `docs/functional-spec/rhei-agents.spec.md:142`, `crates/rhei-cli/src/main.rs:5711`, `crates/rhei-cli/src/main.rs:5779`, `crates/rhei-cli/src/main.rs:5964`, `crates/rhei-cli/src/main.rs:6390`.

10. `covered` — `defaults.agent_mode` is parsed and used as a default mode. Evidence: `docs/functional-spec/rhei-agents.spec.md:143`, `crates/rhei-cli/src/main.rs:5781`, `crates/rhei-cli/src/main.rs:5957`, `crates/rhei-cli/src/main.rs:6418`.

11. `covered` — `defaults.agent_timeout` is parsed and participates in autonomous agent timeout resolution. Evidence: `docs/functional-spec/rhei-agents.spec.md:144`, `crates/rhei-cli/src/main.rs:5785`, `crates/rhei-cli/src/main.rs:5958`, `crates/rhei-cli/src/main.rs:6453`.

12. `partial` — `defaults.program_timeout` is specified, but implementation only supports legacy top-level `program_timeout`; `SettingsDefaults` has no `program_timeout`. Evidence: `docs/functional-spec/rhei-agents.spec.md:145`, `crates/rhei-cli/src/main.rs:5719`, `crates/rhei-cli/src/main.rs:5779`, `crates/rhei-cli/src/main.rs:5968`, `crates/rhei-cli/src/main.rs:6842`.

13. `covered` — `defaults.mcp_servers` and `defaults.skills` are parsed as arrays and used as state defaults. Evidence: `docs/functional-spec/rhei-agents.spec.md:146`, `docs/functional-spec/rhei-agents.spec.md:147`, `crates/rhei-cli/src/main.rs:5788`, `crates/rhei-cli/src/main.rs:5790`, `crates/rhei-cli/src/main.rs:6139`.

14. `covered` — The `agents` registry is keyed by agent id, and state/default references use ids rather than inline state objects. Evidence: `docs/functional-spec/rhei-agents.spec.md:151`, `crates/rhei-cli/src/main.rs:5727`, `crates/rhei-validator/src/lib.rs:130`, `crates/rhei-validator/src/lib.rs:523`.

15. `partial` — Agent profile `command` is required by the spec. The struct has `command: Vec<String>`, but settings deserialization can yield an empty command and `build_agent_command` panics via `expect` instead of producing a validation error. Evidence: `docs/functional-spec/rhei-agents.spec.md:158`, `crates/rhei-validator/src/lib.rs:165`, `crates/rhei-validator/src/lib.rs:167`, `crates/rhei-cli/src/main.rs:6941`.

16. `covered` — Agent `prompt_flag`, `model_flag`, `stdin_prompt`, `timeout`, `mcp_flag`, `mcp_config_flag`, `skill_flag`, and `modes` fields exist in the profile schema. Evidence: `docs/functional-spec/rhei-agents.spec.md:159`, `crates/rhei-validator/src/lib.rs:168`, `crates/rhei-validator/src/lib.rs:171`, `crates/rhei-validator/src/lib.rs:174`, `crates/rhei-validator/src/lib.rs:177`, `crates/rhei-validator/src/lib.rs:180`, `crates/rhei-validator/src/lib.rs:184`, `crates/rhei-validator/src/lib.rs:188`, `crates/rhei-validator/src/lib.rs:192`.

17. `missing` — `mcp_flag` and `mcp_config_flag` are mutually exclusive. No settings/profile validation enforces mutual exclusion for agent profiles. Evidence: `docs/functional-spec/rhei-agents.spec.md:163`, `docs/functional-spec/rhei-agents.spec.md:164`, `crates/rhei-validator/src/lib.rs:180`, `crates/rhei-validator/src/lib.rs:184`, `crates/rhei-cli/src/main.rs:6928`.

18. `missing` — Agent `session` object is part of the agent profile schema. `CustomAgentProfile` has no `session` field, so settings cannot retain or use it. Evidence: `docs/functional-spec/rhei-agents.spec.md:167`, `crates/rhei-validator/src/lib.rs:165`, `crates/rhei-validator/src/lib.rs:196`.

19. `covered` — Built-in agents are preloaded, and user entries with matching ids replace built-ins wholesale. Evidence: `docs/functional-spec/rhei-agents.spec.md:169`, `crates/rhei-cli/src/main.rs:5802`, `crates/rhei-cli/src/main.rs:5930`, `crates/rhei-cli/src/main.rs:5933`, `crates/rhei-cli/src/main.rs:5936`.

20. `partial` — `models` registry is parsed by model id with `provider`, `model`, `default_agent`, and `agents` fields, but required `provider` and `model` are optional in the implementation and are not validated as required. Evidence: `docs/functional-spec/rhei-agents.spec.md:176`, `docs/functional-spec/rhei-agents.spec.md:180`, `docs/functional-spec/rhei-agents.spec.md:181`, `crates/rhei-cli/src/main.rs:5731`, `crates/rhei-cli/src/main.rs:5742`, `crates/rhei-cli/src/main.rs:5745`, `crates/rhei-cli/src/main.rs:5749`.

21. `partial` — `models.<id>.agents.<agent-id>.args`, `autonomous_args`, and `timeout` are parsed, but only `timeout` is consumed; `args` and `autonomous_args` are never appended to the spawned command. Evidence: `docs/functional-spec/rhei-agents.spec.md:185`, `docs/functional-spec/rhei-agents.spec.md:189`, `docs/functional-spec/rhei-agents.spec.md:190`, `crates/rhei-cli/src/main.rs:5760`, `crates/rhei-cli/src/main.rs:5765`, `crates/rhei-cli/src/main.rs:6441`, `crates/rhei-cli/src/main.rs:6950`.

22. `partial` — MCP server profile fields exist, but registry entries are not validated to declare exactly one of `command` or `url`, remote `url` entries are not validated to require `transport`, environment substitutions are not expanded, and no startup timeout handshake is performed. Evidence: `docs/functional-spec/rhei-agents.spec.md:195`, `docs/functional-spec/rhei-agents.spec.md:201`, `docs/functional-spec/rhei-agents.spec.md:202`, `docs/functional-spec/rhei-agents.spec.md:204`, `docs/functional-spec/rhei-agents.spec.md:206`, `docs/functional-spec/rhei-agents.spec.md:208`, `crates/rhei-validator/src/lib.rs:204`, `crates/rhei-cli/src/main.rs:6041`, `crates/rhei-cli/src/main.rs:6066`.

23. `partial` — Skill profile schema includes `path` and `description`, but `~` expansion, existence checks, and readable checks are not implemented for agent spawn. Evidence: `docs/functional-spec/rhei-agents.spec.md:212`, `docs/functional-spec/rhei-agents.spec.md:218`, `docs/functional-spec/rhei-agents.spec.md:219`, `crates/rhei-validator/src/lib.rs:227`, `crates/rhei-cli/src/main.rs:6225`, `crates/rhei-cli/src/main.rs:6068`.

24. `missing` — Resolved skills must be wired only when the agent declares `skill_flag`; otherwise they are skipped with a warning. The command builder does not emit skill flags or warnings at all. Evidence: `docs/functional-spec/rhei-agents.spec.md:221`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6976`, `crates/rhei-cli/src/main.rs:6999`.

25. `partial` — `snapshots` top-level settings block is declared by this spec, but `RheiSettings` has no `snapshots` field in this implementation slice. Snapshot support may exist elsewhere, but this settings surface is not covered here. Evidence: `docs/functional-spec/rhei-agents.spec.md:226`, `crates/rhei-cli/src/main.rs:5709`, `crates/rhei-cli/src/main.rs:5737`.

26. `covered` — Per-state `target`, `all_targets`, legacy `model`, `all_models`, `agent`, `mcp_servers`, and `skills` fields are represented in `StateDef`. Evidence: `docs/functional-spec/rhei-agents.spec.md:233`, `crates/rhei-validator/src/lib.rs:511`, `crates/rhei-validator/src/lib.rs:516`, `crates/rhei-validator/src/lib.rs:518`, `crates/rhei-validator/src/lib.rs:521`, `crates/rhei-validator/src/lib.rs:527`, `crates/rhei-validator/src/lib.rs:553`, `crates/rhei-validator/src/lib.rs:556`.

### Merge Semantics

27. `covered` — Built-in agents load first, then global, then project; user entries replace whole agent profiles by id. Evidence: `docs/functional-spec/rhei-agents.spec.md:244`, `docs/functional-spec/rhei-agents.spec.md:248`, `crates/rhei-cli/src/main.rs:5930`, `crates/rhei-cli/src/main.rs:5933`, `crates/rhei-cli/src/main.rs:5936`.

28. `partial` — `defaults` shallow override by field. Implemented for nested `agent_mode`, `agent_timeout`, `mcp_servers`, and `skills`, and legacy top-level `agent`, `model`, `agent_mode`, `agent_timeout`, `program_timeout`; missing nested `defaults.model`, `defaults.agent`, and `defaults.program_timeout`. Evidence: `docs/functional-spec/rhei-agents.spec.md:247`, `crates/rhei-cli/src/main.rs:5954`, `crates/rhei-cli/src/main.rs:5957`, `crates/rhei-cli/src/main.rs:5963`.

29. `partial` — `models` merge by model id and `models.<id>.agents` merge by agent id. Implementation merges models by replacing the entire `ModelProfile`, so project `models.<id>` replaces global `models.<id>` wholesale instead of merging its inner `agents` map. Evidence: `docs/functional-spec/rhei-agents.spec.md:252`, `docs/functional-spec/rhei-agents.spec.md:253`, `crates/rhei-cli/src/main.rs:5949`, `crates/rhei-cli/src/main.rs:5950`.

30. `covered` — `mcp_servers` and `skills` registries merge by id. Evidence: `docs/functional-spec/rhei-agents.spec.md:254`, `docs/functional-spec/rhei-agents.spec.md:255`, `crates/rhei-cli/src/main.rs:5940`, `crates/rhei-cli/src/main.rs:5945`.

31. `partial` — `null` explicitly clears inherited optional fields. Because settings structs use `Option<T>` without presence tracking, an explicit `null` is indistinguishable from an omitted field for many fields and therefore does not clear inherited values. Evidence: `docs/functional-spec/rhei-agents.spec.md:256`, `crates/rhei-cli/src/main.rs:5957`, `crates/rhei-cli/src/main.rs:5963`.

32. `covered` — `defaults.mcp_servers` and `defaults.skills` are replaced wholesale by project lists, including empty lists. Evidence: `docs/functional-spec/rhei-agents.spec.md:258`, `crates/rhei-cli/src/main.rs:5954`, `crates/rhei-cli/src/main.rs:5959`, `crates/rhei-cli/src/main.rs:5960`.

33. `partial` — A project can override only a model provider/model pair or model-agent binding autonomous args without redefining unrelated global entries. Full partial model merge is not implemented, and `autonomous_args` is parsed but unused. Evidence: `docs/functional-spec/rhei-agents.spec.md:264`, `crates/rhei-cli/src/main.rs:5950`, `crates/rhei-cli/src/main.rs:5765`.

### Resolution Order

34. `partial` — Model id resolution order is CLI override, state-level, project defaults, global defaults. Implementation covers CLI and state, but only legacy top-level merged `settings.model`, not nested project/global `defaults.model`. Evidence: `docs/functional-spec/rhei-agents.spec.md:270`, `crates/rhei-cli/src/main.rs:6378`, `crates/rhei-cli/src/main.rs:6380`, `crates/rhei-cli/src/main.rs:6382`, `crates/rhei-cli/src/main.rs:6385`.

35. `missing` — The resolved model id must exist in the merged `models` registry. Implementation treats unknown model ids as pass-through literals and falls back to the id for `model_name`. Evidence: `docs/functional-spec/rhei-agents.spec.md:278`, `crates/rhei-cli/src/main.rs:6388`, `crates/rhei-cli/src/main.rs:6457`, `crates/rhei-cli/src/main.rs:6458`, `crates/rhei-cli/src/main.rs:6964`.

36. `covered` — If no model is configured, model-specific fields are omitted. Evidence: `docs/functional-spec/rhei-agents.spec.md:278`, `crates/rhei-cli/src/main.rs:6279`, `crates/rhei-cli/src/main.rs:6465`, `crates/rhei-cli/src/main.rs:6983`.

37. `partial` — Autonomous agent id resolution order is CLI, state, project defaults, global defaults, model default. Implementation covers CLI, state, legacy merged top-level `settings.agent`, and model default, but misses nested `defaults.agent`. Evidence: `docs/functional-spec/rhei-agents.spec.md:282`, `crates/rhei-cli/src/main.rs:6390`, `crates/rhei-cli/src/main.rs:6392`, `crates/rhei-cli/src/main.rs:6394`, `crates/rhei-cli/src/main.rs:6396`, `crates/rhei-cli/src/main.rs:6399`.

38. `covered` — Unknown agent id is a configuration error during resolution. Evidence: `docs/functional-spec/rhei-agents.spec.md:291`, `crates/rhei-cli/src/main.rs:6408`, `crates/rhei-cli/src/main.rs:6410`.

39. `partial` — If no agent is configured and the model has no `default_agent`, `rhei run` fails. It does fail, but with a generic "no agent configured" message rather than the model-specific error required by the spec. Evidence: `docs/functional-spec/rhei-agents.spec.md:301`, `crates/rhei-cli/src/main.rs:6404`, `crates/rhei-cli/src/main.rs:7872`, `crates/rhei-cli/src/main.rs:7877`.

40. `partial` — `all_targets` bypasses normal model/agent resolution and uses selector agent/mode/provider/model. Implementation does bypass resolution, but if a target declares a mode and the agent profile has no modes, invalid mode is accepted instead of rejected. Evidence: `docs/functional-spec/rhei-agents.spec.md:309`, `crates/rhei-cli/src/main.rs:6483`, `crates/rhei-cli/src/main.rs:6485`, `crates/rhei-cli/src/main.rs:6300`, `crates/rhei-cli/src/main.rs:6318`.

41. `partial` — Validation must verify that `target`/`all_targets` referenced agents and modes exist. Agent existence is checked, and mode existence is checked only when the profile has non-empty modes. Evidence: `docs/functional-spec/rhei-agents.spec.md:311`, `crates/rhei-cli/src/main.rs:6005`, `crates/rhei-cli/src/main.rs:6014`, `crates/rhei-cli/src/main.rs:6021`.

42. `covered` — Legacy `all_models` runs normal agent resolution independently for each model. Evidence: `docs/functional-spec/rhei-agents.spec.md:313`, `crates/rhei-cli/src/main.rs:6495`, `crates/rhei-cli/src/main.rs:6497`, `crates/rhei-cli/src/main.rs:6498`.

43. `missing` — Snapshot-enabled legacy `model`/`all_models` executions must resolve an effective target tuple before snapshot emit/inherit; otherwise explicit snapshot fields are rejected and auto-emit skipped. No snapshot tuple enforcement is visible in this implementation scope. Evidence: `docs/functional-spec/rhei-agents.spec.md:315`, `crates/rhei-cli/src/main.rs:6253`, `crates/rhei-cli/src/main.rs:6472`.

44. `partial` — Mode resolution order is CLI, state, project defaults, global defaults, first declared registry mode, none. Implementation covers CLI/state and a merged default, but user mode maps are `BTreeMap`, so "first declared mode" is not preserved, and nested/global precedence is collapsed by merge. Evidence: `docs/functional-spec/rhei-agents.spec.md:320`, `crates/rhei-cli/src/main.rs:6418`, `crates/rhei-cli/src/main.rs:6422`, `crates/rhei-cli/src/main.rs:6427`, `crates/rhei-validator/src/lib.rs:196`.

45. `partial` — A resolved mode name must exist when the mode map is non-empty. Covered for non-empty maps, but an explicit mode on an agent with no modes is accepted even though the spec later says it must not be set. Evidence: `docs/functional-spec/rhei-agents.spec.md:332`, `crates/rhei-cli/src/main.rs:6430`, `crates/rhei-cli/src/main.rs:6431`.

46. `covered` — Effective tooling set resolution starts from defaults, unions state list, empty state list clears defaults, deduplicates by id with later/state entries winning, and resolves inline entries as-is. Evidence: `docs/functional-spec/rhei-agents.spec.md:335`, `crates/rhei-cli/src/main.rs:6131`, `crates/rhei-cli/src/main.rs:6161`, `crates/rhei-cli/src/main.rs:6169`, `crates/rhei-cli/src/main.rs:6175`, `crates/rhei-cli/src/main.rs:6207`.

47. `missing` — Tooling id with no registry match and no inline definition is a validation error. The resolver records `definition: None`, but no validation error is emitted before spawn. Evidence: `docs/functional-spec/rhei-agents.spec.md:348`, `crates/rhei-cli/src/main.rs:6221`, `crates/rhei-cli/src/main.rs:6238`, `crates/rhei-cli/src/main.rs:7704`.

48. `covered` — Resolved tooling sets are recomputed per state/invocation/run pass. Evidence: `docs/functional-spec/rhei-agents.spec.md:351`, `crates/rhei-cli/src/main.rs:8294`, `crates/rhei-cli/src/main.rs:8517`.

49. `not-normative` — Partial override example text demonstrates intended resolution rather than adding a separate requirement beyond merge/resolution rules. Evidence: `docs/functional-spec/rhei-agents.spec.md:354`.

### Known And Custom Agent Profiles

50. `covered` — Built-in profiles exist for `claude-code`, `codex`, `gemini`, `cursor`, `kilocode`, and `pi`. Evidence: `docs/functional-spec/rhei-agents.spec.md:380`, `crates/rhei-cli/src/main.rs:5815`, `crates/rhei-cli/src/main.rs:5832`, `crates/rhei-cli/src/main.rs:5851`, `crates/rhei-cli/src/main.rs:5866`, `crates/rhei-cli/src/main.rs:5881`, `crates/rhei-cli/src/main.rs:5898`.

51. `covered` — Built-in prompt delivery, model flags, and `yolo` mode flags match the spec table for the implemented built-ins, and `pi` intentionally has no modes. Evidence: `docs/functional-spec/rhei-agents.spec.md:386`, `crates/rhei-cli/src/main.rs:5818`, `crates/rhei-cli/src/main.rs:5824`, `crates/rhei-cli/src/main.rs:5835`, `crates/rhei-cli/src/main.rs:5840`, `crates/rhei-cli/src/main.rs:5854`, `crates/rhei-cli/src/main.rs:5858`, `crates/rhei-cli/src/main.rs:5884`, `crates/rhei-cli/src/main.rs:5888`, `crates/rhei-cli/src/main.rs:5869`, `crates/rhei-cli/src/main.rs:5873`, `crates/rhei-cli/src/main.rs:5901`.

52. `partial` — Built-in MCP and skill support flags match the table structurally, but command wiring is missing, so those capabilities are not actually passed to subprocesses. Evidence: `docs/functional-spec/rhei-agents.spec.md:388`, `docs/functional-spec/rhei-agents.spec.md:389`, `docs/functional-spec/rhei-agents.spec.md:393`, `crates/rhei-cli/src/main.rs:5822`, `crates/rhei-cli/src/main.rs:5823`, `crates/rhei-cli/src/main.rs:5839`, `crates/rhei-cli/src/main.rs:5905`, `crates/rhei-cli/src/main.rs:6928`.

53. `covered` — Built-in ids overlap with `rhei install-skills --agent` ids. Evidence: `docs/functional-spec/rhei-agents.spec.md:395`, `crates/rhei-cli/src/main.rs:10545`, `crates/rhei-cli/src/main.rs:10548`.

54. `missing` — Built-in snapshot session support/unsupported behavior is not implemented in `CustomAgentProfile.session` because that field is absent. Evidence: `docs/functional-spec/rhei-agents.spec.md:397`, `crates/rhei-validator/src/lib.rs:165`, `crates/rhei-validator/src/lib.rs:196`.

55. `missing` — Agents unsupported for MCP/skills should receive warnings at spawn, and required MCP entries should escalate to errors. No command-builder or run-loop support emits those warnings/errors. Evidence: `docs/functional-spec/rhei-agents.spec.md:404`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6999`.

56. `covered` — Custom agents are declared in the `agents` registry and referenced by id in states/settings; state-side inline agent objects are rejected by the string `AgentConfig` deserializer. Evidence: `docs/functional-spec/rhei-agents.spec.md:412`, `crates/rhei-cli/src/main.rs:5727`, `crates/rhei-validator/src/lib.rs:136`, `crates/rhei-validator/src/lib.rs:523`.

57. `covered` — Mode flags are appended after the base command and before prompt/model flags. Evidence: `docs/functional-spec/rhei-agents.spec.md:456`, `crates/rhei-cli/src/main.rs:6941`, `crates/rhei-cli/src/main.rs:6950`, `crates/rhei-cli/src/main.rs:6958`, `crates/rhei-cli/src/main.rs:6967`.

58. `covered` — When `stdin_prompt` is true, the prompt is written to stdin and `--` is appended after the model flag. Evidence: `docs/functional-spec/rhei-agents.spec.md:466`, `crates/rhei-cli/src/main.rs:6958`, `crates/rhei-cli/src/main.rs:6972`, `crates/rhei-cli/src/main.rs:7334`.

59. `covered` — Rhei does not interpret mode names; it injects the named flag list. Evidence: `docs/functional-spec/rhei-agents.spec.md:470`, `crates/rhei-cli/src/main.rs:6950`.

60. `partial` — An agent may declare no modes and no flags are appended, but `agent_mode` is allowed to remain set on mode-less agents, contrary to the spec. Evidence: `docs/functional-spec/rhei-agents.spec.md:475`, `crates/rhei-cli/src/main.rs:6430`, `crates/rhei-cli/src/main.rs:6950`.

### Prompt Composition

61. `partial` — Prompt has task heading, state, optional personality, instructions, task content, Rhei command guidance, and transitions. Implementation omits the `## Task Content` section when content is empty and renders children under a separate `## Child Tasks` section rather than "task body including child task nodes." Evidence: `docs/functional-spec/rhei-agents.spec.md:480`, `crates/rhei-cli/src/main.rs:6886`, `crates/rhei-cli/src/main.rs:6890`, `crates/rhei-cli/src/main.rs:6893`, `crates/rhei-cli/src/main.rs:6894`, `crates/rhei-cli/src/main.rs:6897`, `crates/rhei-cli/src/main.rs:6909`.

62. `covered` — The prompt tells the spawned agent that `rhei run` advances the task and not to call transition/complete or edit state lines unless launching nested execution. Evidence: `docs/functional-spec/rhei-agents.spec.md:501`, `crates/rhei-cli/src/main.rs:6910`, `crates/rhei-cli/src/main.rs:6913`.

63. `covered` — Prompt carries available transitions from the current state. Evidence: `docs/functional-spec/rhei-agents.spec.md:505`, `crates/rhei-cli/src/main.rs:6867`, `crates/rhei-cli/src/main.rs:6870`.

64. `covered` — Prompt does not add completion prose such as "create every required output artifact and then exit." Evidence: `docs/functional-spec/rhei-agents.spec.md:509`, `crates/rhei-cli/src/main.rs:6886`, `crates/rhei-cli/src/main.rs:6909`.

65. `partial` — Template variables are resolved before prompt delivery, but `{model.provider}` and `{model.name}` are not fully resolved from the `models` registry; provider only comes from target selectors, and name resolves to the model id. Evidence: `docs/functional-spec/rhei-agents.spec.md:517`, `crates/rhei-cli/src/main.rs:6858`, `crates/rhei-cli/src/main.rs:6865`, `crates/rhei-cli/src/main.rs:4615`, `crates/rhei-cli/src/main.rs:4617`, `crates/rhei-cli/src/main.rs:4618`.

66. `covered` — Prompt delivery is via prompt flag or stdin according to agent profile. Evidence: `docs/functional-spec/rhei-agents.spec.md:521`, `crates/rhei-cli/src/main.rs:6958`, `crates/rhei-cli/src/main.rs:6960`, `crates/rhei-cli/src/main.rs:7334`.

### Completion Authority And Condition

67. `not-normative` — The completion authority table defines concepts; the enforceable rules are inventoried below. Evidence: `docs/functional-spec/rhei-agents.spec.md:523`.

68. `covered` — Under worker authority, `rhei run` is not involved; this is existing non-`run` command behavior and no agent-mode code path is entered. Evidence: `docs/functional-spec/rhei-agents.spec.md:541`, `crates/rhei-cli/src/main.rs:551`, `crates/rhei-cli/src/main.rs:293`.

69. `partial` — Under orchestrator authority, the subprocess owns work and `rhei run` owns transition. The prompt instructs agents not to transition or edit state lines, but enforcement is limited to re-reading the plan and respecting external changes after exit. Evidence: `docs/functional-spec/rhei-agents.spec.md:543`, `crates/rhei-cli/src/main.rs:6913`, `crates/rhei-cli/src/main.rs:8389`, `crates/rhei-cli/src/main.rs:8394`.

70. `missing` — `instructions` and `personality` must not describe stopping, transition commands, or completion detection. No validator checks state-machine instruction/personality content for those forbidden topics. Evidence: `docs/functional-spec/rhei-agents.spec.md:548`, `crates/rhei-validator/src/lib.rs:481`, `crates/rhei-validator/src/lib.rs:484`.

71. `partial` — Gating states bypass subprocess spawning and automatic transition. They are skipped because `find_ready_tasks` excludes them, but the required gating log is not emitted because the later logging branch is unreachable for gating states. Evidence: `docs/functional-spec/rhei-agents.spec.md:552`, `crates/rhei-cli/src/main.rs:9141`, `crates/rhei-cli/src/main.rs:7845`.

72. `partial` — Completion condition for agent states is subprocess exit code `0` and every required output artifact exists. Implementation checks outputs before scheduling and after success via pending invocation detection, but missing outputs after success do not produce the required warning and may simply leave the task in place. Evidence: `docs/functional-spec/rhei-agents.spec.md:566`, `crates/rhei-cli/src/main.rs:7884`, `crates/rhei-cli/src/main.rs:8418`, `crates/rhei-cli/src/main.rs:8419`, `crates/rhei-cli/src/main.rs:8436`.

73. `covered` — If a state declares no outputs, output condition is vacuously true and exit success can advance the task. Evidence: `docs/functional-spec/rhei-agents.spec.md:571`, `crates/rhei-cli/src/main.rs:6603`, `crates/rhei-cli/src/main.rs:6614`, `crates/rhei-cli/src/main.rs:8418`, `crates/rhei-cli/src/main.rs:8439`.

74. `covered` — No cross-agent stop signal/sentinel/RPC is implemented; completion uses process exit plus artifacts. Evidence: `docs/functional-spec/rhei-agents.spec.md:574`, `crates/rhei-cli/src/main.rs:7345`, `crates/rhei-cli/src/main.rs:6603`.

75. `partial` — Runtime semantics: wait on exit or timeout, kill direct subprocess, skip artifact check on non-zero, and evaluate transitions on success. The direct subprocess kill is implemented, but non-timeout non-zero exits with any configured timeout are treated as timeout candidates; missing-output warning text is absent. Evidence: `docs/functional-spec/rhei-agents.spec.md:586`, `crates/rhei-cli/src/main.rs:7345`, `crates/rhei-cli/src/main.rs:7354`, `crates/rhei-cli/src/main.rs:7361`, `crates/rhei-cli/src/main.rs:8476`.

76. `partial` — Orchestrator authority requires a finite timeout and missing timeout is a validation error. Implementation enforces this at runtime before spawn, not in validation, and skips the check in dry-run mode. Evidence: `docs/functional-spec/rhei-agents.spec.md:606`, `crates/rhei-cli/src/main.rs:6578`, `crates/rhei-cli/src/main.rs:7912`.

### Environment Variables

77. `covered` — Agent subprocess receives `RHEI_PLAN_PATH`, `RHEI_TASK_ID`, `RHEI_STATE`, and `RHEI_AGENT`. Evidence: `docs/functional-spec/rhei-agents.spec.md:624`, `crates/rhei-cli/src/main.rs:6976`.

78. `covered` — Agent subprocess receives `RHEI_MODEL`, `RHEI_MODEL_PROVIDER`, and `RHEI_MODEL_NAME` when configured/resolved. Evidence: `docs/functional-spec/rhei-agents.spec.md:629`, `crates/rhei-cli/src/main.rs:6983`, `crates/rhei-cli/src/main.rs:6993`, `crates/rhei-cli/src/main.rs:6996`.

79. `partial` — `RHEI_MCP_SERVERS`, `RHEI_MCP_<NAME>_AVAILABLE`, `RHEI_SKILLS`, and `RHEI_SKILL_<ID>_AVAILABLE` are set, but availability only reflects registry/inline resolution rather than real spawn-time availability. Evidence: `docs/functional-spec/rhei-agents.spec.md:633`, `crates/rhei-cli/src/main.rs:7097`, `crates/rhei-cli/src/main.rs:7100`, `crates/rhei-cli/src/main.rs:7106`, `crates/rhei-cli/src/main.rs:6068`.

80. `covered` — Env var id normalization uppercases and replaces non-alphanumeric characters with underscores, covering hyphens and spaces. Evidence: `docs/functional-spec/rhei-agents.spec.md:634`, `crates/rhei-cli/src/main.rs:6109`.

81. `covered` — Agent working directory is workspace root or plan file parent. Evidence: `docs/functional-spec/rhei-agents.spec.md:638`, `crates/rhei-cli/src/main.rs:6945`, `crates/rhei-cli/src/main.rs:8346`, `crates/rhei-cli/src/main.rs:8541`.

### `rhei run` Agent Mode

82. `covered` — CLI supports `--dry-run`, `--no-callbacks`, `--no-agent`, `--agent`, `--model`, `--continue-on-error`, `--parallel`, `--program-timeout`, and `--no-program`. Evidence: `docs/functional-spec/rhei-agents.spec.md:644`, `crates/rhei-cli/src/main.rs:217`, `crates/rhei-cli/src/main.rs:5461`, `crates/rhei-cli/src/main.rs:5491`, `crates/rhei-cli/src/main.rs:5507`.

83. `covered` — CLI also supports `--agent-mode`, which the spec uses elsewhere but omits from the CLI synopsis/table. Evidence: `docs/functional-spec/rhei-agents.spec.md:324`, `crates/rhei-cli/src/main.rs:5498`.

84. `partial` — Sequential execution loop loads and validates, finds ready tasks, resolves model/agent, composes prompt, logs spawn, spawns agent, waits, re-reads plan, respects external state changes, auto-advances on success, warns when no forward transition matches, and applies non-zero error behavior. Gaps: it uses ready tasks rather than `rhei next` claimable tasks, missing-output warning differs, and timeout/non-zero classification is wrong. Evidence: `docs/functional-spec/rhei-agents.spec.md:662`, `crates/rhei-cli/src/main.rs:7704`, `crates/rhei-cli/src/main.rs:7808`, `crates/rhei-cli/src/main.rs:7870`, `crates/rhei-cli/src/main.rs:8311`, `crates/rhei-cli/src/main.rs:8343`, `crates/rhei-cli/src/main.rs:8389`, `crates/rhei-cli/src/main.rs:8439`, `crates/rhei-cli/src/main.rs:8457`, `crates/rhei-cli/src/main.rs:8470`.

85. `partial` — Parallel execution selects up to N tasks and spawns concurrently; `N=0` means all in the current batch. It does not explicitly enforce the spec's transitive `Prior` independence rule beyond readiness and non-concurrent-state filtering, and it waits for all spawned threads in the batch before considering newly claimable tasks rather than spawning as soon as any exits. Evidence: `docs/functional-spec/rhei-agents.spec.md:682`, `crates/rhei-cli/src/main.rs:8252`, `crates/rhei-cli/src/main.rs:8506`, `crates/rhei-cli/src/main.rs:8627`.

86. `partial` — Independence rule: agents that could conflict on the same task file must not be spawned. Single-file fallback is implemented, and non-concurrent state filtering exists, but no direct transitive-prior independence scheduler is implemented. Evidence: `docs/functional-spec/rhei-agents.spec.md:696`, `crates/rhei-cli/src/main.rs:7692`, `crates/rhei-cli/src/main.rs:8213`.

87. `covered` — Single-file plans with `--parallel > 1` warn and fall back to sequential execution. Evidence: `docs/functional-spec/rhei-agents.spec.md:698`, `crates/rhei-cli/src/main.rs:7692`.

88. `covered` — `--no-callbacks`, `--no-agent`, and `--no-program` suppress only their own execution class and can be combined independently. Evidence: `docs/functional-spec/rhei-agents.spec.md:714`, `crates/rhei-cli/src/main.rs:5530`, `crates/rhei-cli/src/main.rs:5562`, `crates/rhei-cli/src/main.rs:5578`, `crates/rhei-cli/src/main.rs:7860`, `crates/rhei-cli/src/main.rs:7873`.

89. `missing` — When a task reaches a gating state, `rhei run` must log `Task {id} is in gating state '{state}'. Waiting for human action.` and continue. Because gating tasks are filtered out of `find_ready_tasks`, this log path is not reached. Evidence: `docs/functional-spec/rhei-agents.spec.md:716`, `crates/rhei-cli/src/main.rs:9141`, `crates/rhei-cli/src/main.rs:7845`.

### Missing Tooling

90. `missing` — Spawn-time availability checks for command MCP servers, URL MCP servers, and skills are not implemented. Availability is currently registry resolution only. Evidence: `docs/functional-spec/rhei-agents.spec.md:728`, `docs/functional-spec/rhei-agents.spec.md:739`, `crates/rhei-cli/src/main.rs:6068`.

91. `missing` — Required tooling failure must prevent spawn and fire `mcp_unavailable`/`skill_unavailable` transition with `triggeredBy: system` and unavailable ids in `transitionData.unavailable`. No such run-loop path exists. Evidence: `docs/functional-spec/rhei-agents.spec.md:754`, `crates/rhei-cli/src/main.rs:8294`, `crates/rhei-cli/src/main.rs:8343`.

92. `missing` — Required tooling failure without a transition must leave the task in place and log `error: required tooling unavailable...`, with `--continue-on-error` handling. No such error path exists. Evidence: `docs/functional-spec/rhei-agents.spec.md:760`, `crates/rhei-cli/src/main.rs:8294`, `crates/rhei-cli/src/main.rs:8487`.

93. `missing` — Optional unavailable tooling must warn, drop the entry before spawn, and reflect availability in prompt/env. Entries are not probed or dropped; unresolved optional entries remain as unavailable by registry only. Evidence: `docs/functional-spec/rhei-agents.spec.md:764`, `crates/rhei-cli/src/main.rs:6068`, `crates/rhei-cli/src/main.rs:7062`, `crates/rhei-cli/src/main.rs:7097`.

94. `missing` — Unsupported agent tooling support must be treated like availability failure: required escalates, optional warns/drops. No unsupported-tooling gate is present, and MCP/skill flags are not emitted at all. Evidence: `docs/functional-spec/rhei-agents.spec.md:770`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6999`.

95. `covered` — State-machine schema accepts and validates `mcp_unavailable` / `skill_unavailable` trigger shapes structurally. Evidence: `docs/functional-spec/rhei-agents.spec.md:776`, `crates/rhei-validator/src/lib.rs:993`, `crates/rhei-validator/src/lib.rs:1390`.

### Timeout Handling

96. `covered` — Agent timeout can resolve from state `agent_timeout`, model-agent binding timeout, agent profile timeout, and defaults. Evidence: `docs/functional-spec/rhei-agents.spec.md:785`, `crates/rhei-cli/src/main.rs:6443`, `crates/rhei-cli/src/main.rs:6446`, `crates/rhei-cli/src/main.rs:6451`, `crates/rhei-cli/src/main.rs:6453`.

97. `partial` — Timeout resolution precedence is state > model-agent binding > agent profile > settings defaults. Covered for parsed fields, but nested `defaults.agent_timeout` and legacy top-level `agent_timeout` both participate, and invalid settings durations silently fall through rather than erroring. Evidence: `docs/functional-spec/rhei-agents.spec.md:824`, `crates/rhei-cli/src/main.rs:6443`, `crates/rhei-cli/src/main.rs:6452`.

98. `partial` — Under orchestrator authority, missing timeout is a validation error. Runtime enforcement exists, but not validator-level enforcement, and dry-run skips it. Evidence: `docs/functional-spec/rhei-agents.spec.md:826`, `crates/rhei-cli/src/main.rs:6578`, `crates/rhei-cli/src/main.rs:7912`.

99. `covered` — Duration parser supports seconds, minutes, hours, and combined units such as `1h30m` and `2h15m30s`. Evidence: `docs/functional-spec/rhei-agents.spec.md:832`, `crates/rhei-validator/src/lib.rs:1640`.

100. `partial` — Timeout behavior sends SIGTERM, waits 10 seconds, then SIGKILL. Covered for direct subprocess, but timeout is not represented distinctly from any other non-zero exit status after a timeout is configured. Evidence: `docs/functional-spec/rhei-agents.spec.md:846`, `crates/rhei-cli/src/main.rs:7352`, `crates/rhei-cli/src/main.rs:7354`, `crates/rhei-cli/src/main.rs:7357`, `crates/rhei-cli/src/main.rs:7361`, `crates/rhei-cli/src/main.rs:8363`.

101. `missing` — Task log must include `agent timed out after {duration}`. No timeout-specific log line is written; only the generic footer is written. Evidence: `docs/functional-spec/rhei-agents.spec.md:850`, `crates/rhei-cli/src/main.rs:7384`.

102. `partial` — On timeout, first matching timeout transition fires with callbacks. A timeout transition function exists, but it is invoked for any non-success status with a configured timeout, ignores transition conditions, and `execute_transition` callback context uses `triggeredBy: user`, not `system`. Evidence: `docs/functional-spec/rhei-agents.spec.md:851`, `crates/rhei-cli/src/main.rs:8476`, `crates/rhei-cli/src/main.rs:9028`, `crates/rhei-cli/src/main.rs:9038`, `crates/rhei-cli/src/main.rs:5226`, `crates/rhei-cli/src/main.rs:5370`.

103. `missing` — If no timeout transition exists, the task remains in current state and the engine logs a warning. `fire_timeout_transition` silently does nothing when no rule exists. Evidence: `docs/functional-spec/rhei-agents.spec.md:853`, `crates/rhei-cli/src/main.rs:9038`, `crates/rhei-cli/src/main.rs:9076`.

104. `missing` — Timeout transition callback receives `triggeredBy: system` and timeout duration in `transitionData.timeout`. `execute_transition` has no way to receive timeout transition data from `fire_timeout_transition`. Evidence: `docs/functional-spec/rhei-agents.spec.md:929`, `crates/rhei-cli/src/main.rs:9029`, `crates/rhei-cli/src/main.rs:5215`, `crates/rhei-cli/src/main.rs:5226`, `crates/rhei-cli/src/main.rs:5370`.

105. `missing` — Snapshot timeout transcripts classified as `completion: timeout` and not preloadable are not covered in this implementation scope. Evidence: `docs/functional-spec/rhei-agents.spec.md:855`, `crates/rhei-cli/src/main.rs:7223`, `crates/rhei-cli/src/main.rs:7384`.

### Log Capture

106. `covered` — Agent stdout and stderr are captured to log files under `runtime/logs`. Evidence: `docs/functional-spec/rhei-agents.spec.md:931`, `crates/rhei-cli/src/main.rs:7114`, `crates/rhei-cli/src/main.rs:7160`, `crates/rhei-cli/src/main.rs:7181`, `crates/rhei-cli/src/main.rs:7240`.

107. `partial` — Log naming covers simple and model-specific states, but counted-loop visit suffix and combined model+visit suffix are not implemented. Evidence: `docs/functional-spec/rhei-agents.spec.md:935`, `crates/rhei-cli/src/main.rs:7114`, `crates/rhei-cli/src/main.rs:6595`, `crates/rhei-cli/src/main.rs:8312`.

108. `covered` — Log header/footer v1 fields include agent, model/provider/name when present, task, state, started, timeout, plan, mcp/skills lines, exit code, duration, and ended. Evidence: `docs/functional-spec/rhei-agents.spec.md:944`, `crates/rhei-cli/src/main.rs:7251`, `crates/rhei-cli/src/main.rs:7257`, `crates/rhei-cli/src/main.rs:7265`, `crates/rhei-cli/src/main.rs:7268`, `crates/rhei-cli/src/main.rs:7271`, `crates/rhei-cli/src/main.rs:7274`, `crates/rhei-cli/src/main.rs:7276`, `crates/rhei-cli/src/main.rs:7277`, `crates/rhei-cli/src/main.rs:7280`, `crates/rhei-cli/src/main.rs:7281`, `crates/rhei-cli/src/main.rs:7287`, `crates/rhei-cli/src/main.rs:7384`.

109. `covered` — Log body is raw agent stdout/stderr written by output readers, and header/footer are added by `rhei run`. Evidence: `docs/functional-spec/rhei-agents.spec.md:963`, `docs/functional-spec/rhei-agents.spec.md:972`, `crates/rhei-cli/src/main.rs:7181`, `crates/rhei-cli/src/main.rs:7256`, `crates/rhei-cli/src/main.rs:7386`.

110. `partial` — `mcp_servers:` and `skills:` entries are resolved ids with `?` for optional failed availability and missing lines when none. Formatting supports `?`, but real availability failure is not implemented, so `?` only reflects unresolved definitions, not spawn-time failed availability. Evidence: `docs/functional-spec/rhei-agents.spec.md:974`, `crates/rhei-cli/src/main.rs:7062`, `crates/rhei-cli/src/main.rs:7077`, `crates/rhei-cli/src/main.rs:7281`.

111. `covered` — `runtime/logs` is created automatically by `rhei run`. Evidence: `docs/functional-spec/rhei-agents.spec.md:979`, `crates/rhei-cli/src/main.rs:7240`.

112. `covered` — `rhei reset` removes the entire `runtime/` directory, including logs. Evidence: `docs/functional-spec/rhei-agents.spec.md:981`, `crates/rhei-cli/src/main.rs:9847`, `crates/rhei-cli/src/main.rs:9871`, `crates/rhei-cli/src/main.rs:9877`.

### Dry Run And Callback-Only Mode

113. `partial` — `rhei run --dry-run` in agent mode shows what would be spawned without executing. It does not execute, but output does not render the actual command line, concrete provider model name, provider/model tuple, or human-formatted timeout as shown by the spec. Evidence: `docs/functional-spec/rhei-agents.spec.md:983`, `crates/rhei-cli/src/main.rs:8257`, `crates/rhei-cli/src/main.rs:8274`, `crates/rhei-cli/src/main.rs:8278`, `crates/rhei-cli/src/main.rs:8735`.

114. `covered` — `rhei run --no-agent` reverts to callback-only advancement without spawning agents. Evidence: `docs/functional-spec/rhei-agents.spec.md:1003`, `crates/rhei-cli/src/main.rs:6479`, `crates/rhei-cli/src/main.rs:7873`, `crates/rhei-cli/src/main.rs:8798`.

## Summary

Major missing or partial areas:

- Spec-shaped nested settings defaults are incomplete: `defaults.model`, `defaults.agent`, and `defaults.program_timeout` are not parsed.
- Model registry validation is missing: required `provider`/`model` and "resolved model id must exist" are not enforced.
- Model-agent `args` / `autonomous_args` are parsed but unused.
- MCP/skill command wiring and spawn-time availability checks are largely missing.
- Timeout handling does not distinguish real timeouts from ordinary non-zero exits, lacks timeout log text, and does not send `triggeredBy: system` or `transitionData.timeout`.
- Completion-condition missing-output warning is not implemented.
- Gating-state logging is unreachable.
- Counted-loop log naming is missing.
- Dry-run output is not spec-shaped.

Transition recommendation: `completeness-aggregate`.
