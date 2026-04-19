# `rhei install-skills`

Install rhei skills (plan-writer, plan-worker) into the configuration directories of major AI coding agents, so any agent session can invoke them without per-project setup.

## Usage

```
rhei install-skills [OPTIONS]
rhei install-skills --agent claude-code
rhei install-skills --agent cursor
rhei install-skills --agent all
rhei install-skills --uninstall --agent claude-code
```

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--agent <NAME>` | `all` | Target agent: `claude-code`, `cursor`, `windsurf`, `copilot`, `cline`, `aider`, or `all` |
| `--link` | (default) | Symlink skill files (stays up-to-date with rhei releases) |
| `--copy` | | Copy skill files instead of symlinking |
| `--uninstall` | | Remove previously installed skills |
| `--dry-run` | | Print what would be done without changing anything |
| `--skills <LIST>` | `plan-writer,plan-worker,state-machine-writer` | Comma-separated list of skills to install |

## Agent Targets

Each agent has a different configuration layout. The command handles each one.

### Claude Code (`claude-code`)

**Skill files:** Symlink `skills/rhei-plan-writer/`, `skills/rhei-plan-worker/`, and `skills/rhei-state-machine-writer/` into `~/.claude/skills/`.

**Registration:** Append a section to `~/.claude/CLAUDE.md`:

```markdown
# rhei
- **rhei-plan-writer** (`~/.claude/skills/rhei-plan-writer/SKILL.md`) — create and validate Rhei Plans. Trigger: `/rhei-plan-writer`
- **rhei-plan-worker** (`~/.claude/skills/rhei-plan-worker/SKILL.md`) — execute tasks in a Rhei Plan. Trigger: `/rhei-plan-worker <plan>`
- **rhei-state-machine-writer** (`~/.claude/skills/rhei-state-machine-writer/SKILL.md`) — design custom state machines from project specs and teams. Trigger: `/rhei-state-machine-writer`
When the user types `/rhei-plan-writer`, `/rhei-plan-worker`, or `/rhei-state-machine-writer`, invoke the Skill tool with the corresponding skill name before doing anything else.
```

### Cursor (`cursor`)

**Skill files:** Copy skill content into `~/.cursor/rules/rhei-plan-writer.mdc` and `~/.cursor/rules/rhei-plan-worker.mdc`.

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

### Windsurf (`windsurf`)

**Skill files:** Append skill instructions to `~/.windsurfrules` (or `~/.codeium/windsurf/memories/global_rules.md` depending on version).

**Format:** Plain markdown sections, delimited with `<!-- rhei:start -->` / `<!-- rhei:end -->` markers for clean uninstall.

### GitHub Copilot (`copilot`)

**Skill files:** Write to `~/.github/copilot-instructions.md` (global) or offer project-level `.github/copilot-instructions.md`.

**Format:** Plain markdown appended between `<!-- rhei:start -->` / `<!-- rhei:end -->` markers.

**Note:** Copilot's instruction file has no skill/trigger system — the content is injected as system context. Skills are presented as "when the user asks to create/execute a Rhei plan, follow these instructions."

### Cline (`cline`)

**Skill files:** Write to `~/.cline/rules/rhei-plan-writer.md` and `~/.cline/rules/rhei-plan-worker.md`.

**Format:** Plain markdown with Cline's frontmatter if supported, otherwise raw content.

### Aider (`aider`)

**Skill files:** Add `read:` entries in `~/.aider.conf.yml` pointing at the skill markdown:

```yaml
read:
  - ~/.local/share/rhei/skills/rhei-plan-writer/SKILL.md
  - ~/.local/share/rhei/skills/rhei-plan-worker/SKILL.md
```

## Behavior

### Detect installed skills

Before writing, check if rhei skills are already installed for the target agent. If so, print "already installed" and skip. Combine with `--copy` or `--link` to force an update.

### Resolve skill source

The command finds skill files relative to the `rhei` binary (e.g., `../share/rhei/skills/` for installed binaries, or `skills/` in the repo for dev builds).

### Symlink vs copy

Default is `--link`, which symlinks to the source. `--copy` copies the files — necessary when the rhei repo is not persistently available (e.g., installed via `cargo install`).

### Registration

For agents that require explicit registration (Claude Code's `CLAUDE.md`), the command appends a delimited section. It uses markers (`<!-- rhei:start -->` / `<!-- rhei:end -->` or an `# rhei` heading) so uninstall and updates can find and replace the block idempotently.

### Dry run

With `--dry-run`, print each action (symlink, copy, append) without executing.

### Uninstall

With `--uninstall`, remove symlinks/copied files and delete the registered section from agent config files.

## Example Output

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

cline:
  ✓ ~/.cline/rules/rhei-plan-writer.md — written
  ✓ ~/.cline/rules/rhei-plan-worker.md — written

aider:
  ✓ ~/.aider.conf.yml — added 2 read entries

Installed rhei skills for 6 agents.
```

## Implementation Notes

- New `InstallSkills` variant in the `Commands` enum in `crates/rhei-cli/src/main.rs`.
- Agent-specific logic should be a match arm per agent, keeping format conversion isolated.
- The `.mdc` conversion for Cursor and marker-delimited injection for Windsurf/Copilot are the only non-trivial transforms — all others are copy/symlink plus optional registration.

## Related Documentation

- [Plan Language Specification](../rhei.spec.md) - Formal grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
