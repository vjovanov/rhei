# FS-rhei-install-skills: `rhei install-skills`

Install rhei skills (plan-writer, plan-worker, state-machine-writer) into the configuration directories of major AI coding agents, so any agent session can invoke them without per-project setup. Supports both global (user-level) and project-local installation.

## 1. Usage

```
rhei install-skills [OPTIONS]
rhei install-skills --agent claude-code
rhei install-skills --agent cursor
rhei install-skills --agent all
rhei install-skills --local --agent claude-code
rhei install-skills --uninstall --agent claude-code
```

## 2. Options

| Flag | Default | Description |
|------|---------|-------------|
| `--agent <NAME>` | `all` | Target agent: `claude-code`, `cursor`, `windsurf`, `copilot`, `kilocode`, `pi`, `codex`, `antigravity`, or `all` |
| `--local` | | Install into the current project directory instead of global user config |
| `--link` | | Symlink skill files instead of copying (stays up-to-date with rhei releases) |
| `--uninstall` | | Remove previously installed skills |
| `--dry-run` | | Print what would be done without changing anything |
| `--skills <LIST>` | `rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer` | Comma-separated list of skills to install |

## 3. Agent Targets

Each agent has a different configuration layout. The command handles each one. The tables below show global (default) and project-local (`--local`) paths.

### 3.1. Claude Code (`claude-code`)

| Mode | Skill files | Registration |
|------|-------------|--------------|
| Global | `~/.claude/skills/rhei-<skill>/` | `~/.claude/CLAUDE.md` |
| Local | `.claude/skills/rhei-<skill>/` | `.claude/CLAUDE.md` (project root) |

**Registration:** Append a section to the target `CLAUDE.md`:

```markdown
# rhei
- **rhei-plan-writer** (`~/.claude/skills/rhei-plan-writer/SKILL.md`) — create and validate Rhei Plans. Trigger: `/rhei-plan-writer`
- **rhei-plan-worker** (`~/.claude/skills/rhei-plan-worker/SKILL.md`) — execute tasks in a Rhei Plan. Trigger: `/rhei-plan-worker <plan>`
- **rhei-state-machine-writer** (`~/.claude/skills/rhei-state-machine-writer/SKILL.md`) — design custom state machines from project specs and teams. Trigger: `/rhei-state-machine-writer`
When the user types `/rhei-plan-writer`, `/rhei-plan-worker`, or `/rhei-state-machine-writer`, invoke the Skill tool with the corresponding skill name before doing anything else.
```

In local mode, the paths in the registration block use relative paths (e.g., `.claude/skills/rhei-plan-writer/SKILL.md`).

### 3.2. Cursor (`cursor`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.cursor/rules/rhei-<skill>.mdc` |
| Local | `.cursor/rules/rhei-<skill>.mdc` (project root) |

**Format:** Cursor uses `.mdc` files with YAML frontmatter:

```markdown
---
description: Create and validate Rhei Plan markdown documents
globs:
  - "**/*.rhei.md"
alwaysApply: false
---

<SKILL.md content>
```

### 3.3. Windsurf (`windsurf`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.windsurfrules` (or `~/.codeium/windsurf/memories/global_rules.md`) |
| Local | `.windsurfrules` (project root) |

**Format:** Plain markdown sections, delimited with `<!-- rhei:start -->` / `<!-- rhei:end -->` markers for clean uninstall.

### 3.4. GitHub Copilot (`copilot`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.github/copilot-instructions.md` |
| Local | `.github/copilot-instructions.md` (project root) |

**Format:** Plain markdown appended between `<!-- rhei:start -->` / `<!-- rhei:end -->` markers.

**Note:** Copilot's instruction file has no skill/trigger system — the content is injected as system context. Skills are presented as "when the user asks to create/execute a Rhei plan, follow these instructions."

### 3.5. Kilocode (`kilocode`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.kilocode/rules/rhei-<skill>.md` |
| Local | `.kilocode/rules/rhei-<skill>.md` (project root) |

**Format:** Plain markdown with Kilocode's frontmatter if supported, otherwise raw content.

### 3.6. Pi (`pi`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.pi/rules/rhei-<skill>.md` |
| Local | `.pi/rules/rhei-<skill>.md` (project root) |

**Format:** Plain markdown rule files. Pi loads all `.md` files from its rules directory as system context.

### 3.7. OpenAI Codex (`codex`)

| Mode | Skill files | Registration |
|------|-------------|--------------|
| Global | `~/.agents/skills/rhei-<skill>/SKILL.md` | None |
| Local | `.agents/skills/rhei-<skill>/SKILL.md` (project root) | None |

**Format:** A standard Codex skill directory containing `SKILL.md` and any optional supporting files (`scripts/`, `references/`, `assets/`, `agents/`).

**Note:** Codex discovers skills by scanning `.agents/skills` from the current working directory up to the repository root, plus `$HOME/.agents/skills` for user-level skills. No registration or marker injection file is needed. Custom spawned agents are configured separately under `.codex/agents/*.toml` or `~/.codex/agents/*.toml`; they inherit the parent session's available skills unless `skills.config` is explicitly overridden.

### 3.8. Google Antigravity (`antigravity`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.antigravity/rules/rhei-<skill>.md` |
| Local | `.antigravity/rules/rhei-<skill>.md` (project root) |

**Format:** Plain markdown rule files.

## 4. Behavior

### 4.1. Local installation

With `--local`, skills are installed into the current project directory instead of the user's home directory. The command resolves the project root by walking up from the current directory to find a `.git` directory, `Cargo.toml`, `package.json`, or similar project marker. If no project root is found, it falls back to the current working directory.

Local installation is useful for:

- Sharing skills with collaborators via version control (the default copies files).
- Scoping skills to a specific project without polluting the global config.
- Overriding global skills with project-specific versions.

When `--local` is combined with `--link`, the symlinks use relative paths so the project stays portable. Files installed with `--local` and `--link` should be added to `.gitignore` unless the intent is to commit them.

### 4.2. Detect installed skills

Before writing, remove or replace any existing rhei skill files for the target agent and install the requested set again. Re-running `install-skills` refreshes previously installed skills in place instead of skipping.

### 4.3. Resolve skill source

The command finds skill files relative to the `rhei` binary (e.g., `../share/rhei/skills/` for installed binaries, or `skills/` in the repo for dev builds).

### 4.4. Symlink vs copy

The default behavior copies skill files into the target directory. `--link` symlinks instead — useful during development so skills stay up-to-date with local changes, but requires the rhei source to remain at a stable path.

### 4.5. Registration

For agents that require explicit registration (Claude Code's `CLAUDE.md`), the command appends a delimited section. It uses markers (`<!-- rhei:start -->` / `<!-- rhei:end -->` or an `# rhei` heading) so uninstall and updates can find and replace the block idempotently.

### 4.6. Dry run

With `--dry-run`, print each action (symlink, copy, append) without executing.

### 4.7. Uninstall

With `--uninstall`, remove symlinks/copied files and delete the registered section from agent config files.

## Example Output

### Global (default)

```
$ rhei install-skills --agent all

claude-code:
  ✓ ~/.claude/skills/rhei-plan-writer → /usr/share/rhei/skills/rhei-plan-writer
  ✓ ~/.claude/skills/rhei-plan-worker → /usr/share/rhei/skills/rhei-plan-worker
  ✓ ~/.claude/CLAUDE.md — registered 2 skills

cursor:
  ✓ ~/.cursor/rules/rhei-plan-writer.mdc — written
  ✓ ~/.cursor/rules/rhei-plan-worker.mdc — written

windsurf:
  ✓ ~/.windsurfrules — appended rhei section

copilot:
  ✓ ~/.github/copilot-instructions.md — appended rhei section

kilocode:
  ✓ ~/.kilocode/rules/rhei-plan-writer.md — written
  ✓ ~/.kilocode/rules/rhei-plan-worker.md — written

pi:
  ✓ ~/.pi/rules/rhei-plan-writer.md — written
  ✓ ~/.pi/rules/rhei-plan-worker.md — written

codex:
  ✓ ~/.agents/skills/rhei-plan-writer — copied
  ✓ ~/.agents/skills/rhei-plan-worker — copied

antigravity:
  ✓ ~/.antigravity/rules/rhei-plan-writer.md — written
  ✓ ~/.antigravity/rules/rhei-plan-worker.md — written

Installed rhei skills for 8 agents.
```

### Project-local

```text
$ rhei install-skills --local --agent claude-code

claude-code (local):
  ✓ .claude/skills/rhei-plan-writer → ../../target/rhei/skills/rhei-plan-writer
  ✓ .claude/skills/rhei-plan-worker → ../../target/rhei/skills/rhei-plan-worker
  ✓ .claude/skills/rhei-state-machine-writer → ../../target/rhei/skills/rhei-state-machine-writer
  ✓ .claude/CLAUDE.md — registered 3 skills

Installed rhei skills locally for 1 agent.
```

## Implementation Notes

- New `InstallSkills` variant in the `Commands` enum in `crates/rhei-cli/src/main.rs`.
- Agent-specific logic should be a match arm per agent, keeping format conversion isolated.
- The `.mdc` conversion for Cursor and marker-delimited injection for Windsurf/Copilot are the only non-trivial transforms — all others are copy/symlink plus optional registration.

## Related Documentation

- [Plan Language Specification](rhei-plan-language.spec.md) - Formal grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
