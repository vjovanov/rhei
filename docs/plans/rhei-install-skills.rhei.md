# Rhei: Implement `rhei install-skills` command

## Context

Add a new `install-skills` subcommand to the rhei CLI that copies (or symlinks) rhei skill files into the configuration directories of major AI coding agents. Supports 8 agents: Claude Code, Cursor, Windsurf, Copilot, Kilocode, Pi, Codex, and Antigravity. Supports both global (user-level) and project-local (`--local`) installation.

The full specification lives in `docs/specs/rhei-install-skills.spec.md`.

Skill source files are in `skills/` (rhei-plan-writer, rhei-plan-worker, rhei-state-machine-writer). The CLI lives in `crates/rhei-cli/src/main.rs` and uses clap derive macros for command dispatch. Each command is a variant in the `Commands` enum with a standalone handler function returning `MietteResult<()>`.

## Tasks

### Task 1: Add `InstallSkills` command variant and CLI argument parsing
**State:** completed

Add a new `InstallSkills` variant to the `Commands` enum in `crates/rhei-cli/src/main.rs` with clap-derived fields:

- `--agent <NAME>` (default `all`): enum or string accepting `claude-code`, `cursor`, `windsurf`, `copilot`, `kilocode`, `pi`, `codex`, `antigravity`, `all`.
- `--local`: bool flag.
- `--link`: bool flag.
- `--uninstall`: bool flag.
- `--dry-run`: bool flag.
- `--skills <LIST>`: comma-separated list, default `rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer`.

Define an `Agent` enum with variants for each supported agent and an `All` variant. Wire the new variant into the `main()` match arm, calling a stub `install_skills_command()` function.

#### Task 1.1: Define Agent enum and InstallSkills clap struct
**State:** `completed`

Create the `Agent` enum (with `clap::ValueEnum` derive) and the `InstallSkills` struct with all flags. Add the variant to `Commands`.

Added `Agent` enum with 9 variants (`ClaudeCode`, `Cursor`, `Windsurf`, `Copilot`, `Kilocode`, `Pi`, `Codex`, `Antigravity`, `All`) deriving `clap::ValueEnum`. Added `InstallSkills` variant to the `Commands` enum with all six flags (`--agent`, `--local`, `--link`, `--uninstall`, `--dry-run`, `--skills`) matching the spec. The `--skills` flag uses `value_delimiter = ','` for comma-separated input.

#### Task 1.2: Wire command dispatch
**State:** `completed`

Add the match arm in `main()` that calls `install_skills_command()`. The stub function should just print "not yet implemented" and return `Ok(())`.

Added the `Commands::InstallSkills` match arm in `run()` dispatching to `install_skills_command()`. The stub prints "install-skills: not yet implemented" and returns `Ok(())`. Also added `install-skills` to the help template under a new "Setup:" section. All 215 existing tests pass.

### Task 2: Implement skill source resolution
**State:** completed
**Prior:** Task 1

Implement a function `resolve_skill_source(skill_name: &str) -> MietteResult<PathBuf>` that locates skill source directories. Search relative to the `rhei` binary: first `../share/rhei/skills/<skill_name>/`, then fall back to `skills/<skill_name>/` relative to the repo root (for dev builds). Return an error if neither path exists.

#### Task 2.1: Implement binary-relative resolution
**State:** `completed`

Use `std::env::current_exe()` to find the binary location, then check `../share/rhei/skills/<skill>/`.

Implemented in `resolve_skill_source()`: uses `std::env::current_exe()` to get the binary path, checks `../share/rhei/skills/<skill_name>/` relative to the binary directory. Returns a canonicalized path on success.

#### Task 2.2: Implement dev-build fallback
**State:** `completed`

If the installed path doesn't exist, try the `skills/` directory in the repo root (walk up from binary looking for `Cargo.toml`).

Implemented as the second search path in `resolve_skill_source()`: walks up from the binary directory looking for `Cargo.toml`, then checks `skills/<skill_name>/` relative to that root. Returns a clear error message listing both searched paths if neither exists.

### Task 3: Implement project root detection for `--local`
**State:** completed
**Prior:** Task 1

Implement `find_project_root() -> MietteResult<PathBuf>` that walks up from the current directory looking for `.git`, `Cargo.toml`, `package.json`, or similar project markers. Fall back to the current working directory if no marker is found.

#### Task 3.1: Implement marker-based walk
**State:** `completed`

Walk up parent directories checking for project markers. Return the first directory containing one.

Implemented in `find_project_root()`: walks up from `std::env::current_dir()` checking for `.git`, `Cargo.toml`, `package.json`, `pyproject.toml`, and `go.mod`.

#### Task 3.2: Handle fallback to cwd
**State:** `completed`

If no marker is found, return the current working directory.

Falls back to `std::env::current_dir()` when no marker is found in any ancestor directory.

### Task 4: Implement copy and symlink operations
**State:** completed
**Prior:** Task 2

Implement the core file operations: `copy_skill(src: &Path, dest: &Path, dry_run: bool)` and `link_skill(src: &Path, dest: &Path, dry_run: bool)`. Both should create parent directories as needed, print what they're doing, and skip the actual operation when `dry_run` is true. For `--link`, use relative symlink paths when installing locally.

#### Task 4.1: Implement copy operation
**State:** `completed`

Recursively copy a skill directory to the destination. Create parent dirs. Print `✓ <dest> — written` on success.

Implemented `copy_skill()` and `copy_dir_recursive()`: removes existing destination if present, recursively copies all files and subdirectories, prints `✓ <dest> — written`.

#### Task 4.2: Implement symlink operation
**State:** `completed`

Create a symlink from dest to src. For local installs, compute and use relative paths. Print `✓ <dest> → <src>` on success.

Implemented `link_skill()`: creates parent dirs, removes existing symlink/dir at dest, creates Unix symlink. Also implemented `relative_path()` helper for computing relative paths between directories.

#### Task 4.3: Implement dry-run mode
**State:** `completed`

When dry_run is true, print what would happen without creating files or directories.

Both `copy_skill()` and `link_skill()` check `dry_run` first and print `[dry-run]` prefixed messages without performing any I/O.

### Task 5: Implement marker-delimited text injection
**State:** completed
**Prior:** Task 1

Implement a utility for appending and removing delimited content blocks in text files, used by agents that inject into a shared config file (Windsurf, Copilot, Codex, Claude Code's CLAUDE.md).

#### Task 5.1: Implement marker injection
**State:** `completed`

Write `inject_marked_section(file: &Path, content: &str, dry_run: bool)` that appends content between `<!-- rhei:start -->` / `<!-- rhei:end -->` markers. If markers already exist, replace the content between them. Create the file if it doesn't exist.

Implemented `inject_marked_section()`: creates parent dirs, reads existing content (or starts empty), replaces between markers if found or appends the block. Supports dry-run mode.

#### Task 5.2: Implement marker removal
**State:** `completed`

Write `remove_marked_section(file: &Path, dry_run: bool)` that finds and removes the `<!-- rhei:start -->` to `<!-- rhei:end -->` block, including the markers. For Claude Code, also handle the `# rhei` heading-based block.

Implemented `remove_marked_section()`: removes `<!-- rhei:start/end -->` blocks and also handles `# rhei` / `## rhei` heading blocks by scanning for the heading and consuming lines until a same-or-higher-level heading is found. No-ops if file doesn't exist or no markers found.

### Task 6: Implement detection of already-installed skills
**State:** completed
**Prior:** Task 4, Task 5

Implement installation so existing rhei skill files are replaced in place. If a prior install exists, remove or overwrite the existing files and write the requested set again rather than skipping the agent.

#### Task 6.1: Check file-based agents
**State:** `completed`

For agents that use individual files (Cursor, Kilocode, Pi, Antigravity), check if the skill files exist at the expected paths.

Implemented in `is_agent_installed()` with per-agent match arms. Checks existence of the first skill file at the expected rules directory path.

#### Task 6.2: Check marker-based agents
**State:** `completed`

For agents that inject into shared files (Windsurf, Copilot, Codex), check for the presence of rhei markers in the config file.

Uses `has_rhei_markers()` helper that reads the file and checks for `<!-- rhei:start -->` marker.

#### Task 6.3: Check Claude Code
**State:** `completed`

Check for both skill directories and the `# rhei` registration block in `CLAUDE.md`.

Checks for the first skill directory under `~/.claude/skills/` (or `.claude/skills/` for local). The `install_agent()` function skips installation when detected unless `--link` forces an update.

### Task 7: Implement Claude Code agent handler
**State:** completed
**Prior:** Task 4, Task 5, Task 6

Implement `install_claude_code(skills: &[String], local: bool, link: bool, dry_run: bool)`. Copy or symlink each skill directory into `~/.claude/skills/` (or `.claude/skills/` for local). Append the registration block to `CLAUDE.md` using the `# rhei` heading as delimiter. In local mode, use relative paths in the registration block.

#### Task 7.1: Install skill directories
**State:** `completed`

Copy or symlink each skill into the target skills directory.

Implemented in `install_claude_code()`. Uses `copy_skill()`/`link_skill()` to install each skill dir to `~/.claude/skills/<name>/` or `.claude/skills/<name>/`. For local `--link`, uses `relative_path()` for portable symlinks.

#### Task 7.2: Generate and inject registration block
**State:** `completed`

Build the `# rhei` markdown block with skill entries and triggers. Append to `CLAUDE.md`. Adjust paths for local vs global mode.

Generates a `# rhei` heading block with skill entries and trigger instructions. Uses `inject_claude_md_section()` which replaces existing `# rhei` blocks or appends. Paths adjust for local vs global (`~/.claude/` vs `.claude/`).

### Task 8: Implement Cursor agent handler
**State:** completed
**Prior:** Task 4, Task 6

Implement `install_cursor(skills: &[String], local: bool, link: bool, dry_run: bool)`. For each skill, read the `SKILL.md` content and wrap it in a `.mdc` file with YAML frontmatter (`description`, `globs`, `alwaysApply: false`). Write to `~/.cursor/rules/` or `.cursor/rules/`.

#### Task 8.1: Implement MDC format conversion
**State:** `completed`

Read `SKILL.md`, extract a suitable description, and wrap in `.mdc` frontmatter format.

Implemented in `install_cursor()`. Reads `SKILL.md`, wraps with YAML frontmatter (description from `skill_description()`, globs `**/*.rhei.md`, alwaysApply false).

#### Task 8.2: Write MDC files
**State:** `completed`

Write the converted files to the appropriate rules directory.

Writes `.mdc` files to `~/.cursor/rules/` or `.cursor/rules/`. Creates the rules directory if needed.

### Task 9: Implement simple file-copy agents (Kilocode, Pi, Antigravity)
**State:** completed
**Prior:** Task 4, Task 6

Implement handlers for agents that just need plain markdown files copied into a rules directory. These share the same pattern: copy `SKILL.md` content to `<config>/rules/rhei-<skill>.md`. Factor into a shared helper that takes the config directory path.

#### Task 9.1: Implement shared rules-directory handler
**State:** `completed`

Write a helper function that copies or symlinks skill files into a given rules directory path.

Implemented `install_rules_dir_agent()` — a shared helper parameterized by config dir name. Copies SKILL.md content or creates symlinks to `<config>/rules/<name>.md`.

#### Task 9.2: Wire Kilocode, Pi, and Antigravity
**State:** `completed`

Call the shared helper with the appropriate config paths for each agent (`~/.kilocode/rules/`, `~/.pi/rules/`, `~/.antigravity/rules/`, and their local equivalents).

Wired in `install_agent()` dispatch: Kilocode → `.kilocode`, Pi → `.pi`, Antigravity → `.antigravity`.

### Task 10: Implement marker-injection agents (Windsurf, Copilot, Codex)
**State:** completed
**Prior:** Task 5, Task 6

Implement handlers for agents that inject skill content between markers in a shared config file. Use the marker injection utility from Task 5.

#### Task 10.1: Implement Windsurf handler
**State:** `completed`

Inject into `~/.windsurfrules` or `.windsurfrules`. Check for the alternative global path `~/.codeium/windsurf/memories/global_rules.md`.

Implemented `install_windsurf()`. Checks for alt path `~/.codeium/windsurf/memories/global_rules.md` first, falls back to `~/.windsurfrules`. Uses `inject_marked_section()`.

#### Task 10.2: Implement Copilot handler
**State:** `completed`

Inject into `~/.github/copilot-instructions.md` or `.github/copilot-instructions.md`.

Implemented `install_copilot()`. Injects between markers in the copilot instructions file.

#### Task 10.3: Implement Codex handler
**State:** `completed`

Install standard Codex skill directories under `~/.agents/skills/` or `.agents/skills/` so Codex discovers them automatically. Do not inject anything into `.codex/instructions.md`; custom agent overrides live separately under `.codex/agents/*.toml`.

Implemented `install_codex()`. Copies/symlinks each skill directory to `~/.agents/skills/` or `.agents/skills/`, which matches Codex's documented skill discovery paths. No registration file is written because Codex scans those directories automatically; per-agent customization is handled by `.codex/agents/*.toml` and optional `skills.config` overrides.

### Task 11: Implement uninstall flow
**State:** completed
**Prior:** Task 7, Task 8, Task 9, Task 10

Implement `--uninstall` for all agents. Remove copied/symlinked skill files and delete marker-delimited sections from config files. Each agent handler should have an uninstall path that reverses its install actions.

#### Task 11.1: Uninstall file-based agents
**State:** `completed`

Remove skill files/directories for Claude Code, Cursor, Kilocode, Pi, Antigravity, and Codex.

Implemented in `uninstall_agent()` with per-agent match arms. Uses `remove_path()` to delete skill files/directories for each agent.

#### Task 11.2: Uninstall marker sections
**State:** `completed`

Remove `<!-- rhei:start -->` / `<!-- rhei:end -->` blocks from Windsurf, Copilot, and Codex config files. Remove `# rhei` block from Claude Code's `CLAUDE.md`.

Uses `remove_marked_section()` (from Task 5) which handles both HTML markers and `# rhei` heading blocks.

### Task 12: Implement main orchestrator and output formatting
**State:** completed
**Prior:** Task 7, Task 8, Task 9, Task 10, Task 11

Implement the `install_skills_command()` body. Resolve the agent list (expand `all`), iterate agents, call each handler, collect results, and print the summary output matching the spec's example format (agent name, indented results with `✓` marks, final count line).

#### Task 12.1: Implement agent pass and dispatch
**State:** `completed`

Expand `all` to the full agent list. Loop through agents, call the appropriate handler, and collect success/skip/error status.

Implemented `expand_agent_list()` and `install_skills_command()` orchestrator. Iterates agents, resolves skill sources up front, dispatches to `install_agent()` or `uninstall_agent()`.

#### Task 12.2: Implement formatted output
**State:** `completed`

Print per-agent results with indented `✓` lines and a final summary line (`Installed rhei skills for N agents.`).

Each agent handler prints indented `✓` lines. The orchestrator prints the agent label header and a final summary line with count. Supports `(local)` suffix and `Uninstalled`/`Installed` verb.

### Task 13: Add integration tests
**State:** completed
**Prior:** Task 12

Add integration tests in `crates/rhei-cli/tests/` that exercise the install-skills command against a temporary directory.

#### Task 13.1: Test global install with copy (default)
**State:** `completed`

Set `HOME` to a temp dir, run `install-skills --agent claude-code`, verify files are copied and `CLAUDE.md` has the registration block.

Implemented `global_install_copy_claude_code` test. Verifies skill directories, SKILL.md files, CLAUDE.md content, and output formatting.

#### Task 13.2: Test local install
**State:** `completed`

Run `install-skills --local --agent cursor` from a temp project dir, verify `.cursor/rules/` files are created.

Implemented `local_install_cursor` test. Creates a temp project with Cargo.toml marker, verifies .mdc files and frontmatter format.

#### Task 13.3: Test `--link` mode
**State:** `completed`

Run with `--link`, verify symlinks are created instead of copies.

Implemented `link_mode_creates_symlinks` test. Verifies symlink metadata for kilocode agent.

#### Task 13.4: Test `--uninstall`
**State:** `completed`

Install then uninstall, verify all files and marker sections are removed.

Implemented `uninstall_removes_files` test. Installs then uninstalls claude-code, verifies directories are removed.

#### Task 13.5: Test `--dry-run`
**State:** `completed`

Run with `--dry-run`, verify output is printed but no files are created.

Implemented `dry_run_does_not_create_files` test. Verifies `[dry-run]` output and no file creation.

#### Task 13.6: Test idempotency
**State:** `completed`

Run install twice, mutate one installed file between runs, and verify the second run restores the expected content without duplicating registration blocks.

Implemented `reinstall_overwrites_existing_skill_files` test. Runs install twice, corrupts an installed skill file between runs, and verifies the second run restores the packaged skill content.
