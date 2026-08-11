// Skills compiled into the binary, so `rhei install-skills` works from a bare
// `cargo install` with no companion asset directory to find.
// §FS-rhei-install-skills.4.3

use include_dir::{include_dir, Dir};

/// The shipped skill library, embedded at compile time.
///
/// Living under the CLI package (rather than the repo root) is what makes these
/// files part of the published crate: `cargo install` extracts the package, and
/// a directory outside it would simply not be there.
static BUILTIN_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/skills");

/// Names of the built-in skills, in listing order. This is also the default
/// `--skills` set: everything the binary carries installs unless narrowed.
/// §FS-rhei-install-skills.2
pub(crate) fn builtin_skill_names() -> Vec<String> {
    let mut names: Vec<String> = BUILTIN_SKILLS
        .dirs()
        .filter_map(|dir| dir.path().file_name()?.to_str().map(ToOwned::to_owned))
        .collect();
    names.sort();
    names
}

/// Whether a built-in skill of this name exists.
pub(crate) fn builtin_skill_exists(name: &str) -> bool {
    BUILTIN_SKILLS.get_dir(name).is_some()
}

/// Extract a built-in skill into `root` and return the skill directory.
///
/// The install pipeline reads skills from the filesystem — `SKILL.md` plus
/// whatever `references/` a skill carries. Rather than teach every agent
/// backend to read from two kinds of source, an embedded skill is materialized
/// once and then behaves exactly like a checkout's copy. `root` must outlive
/// the use of the returned path.
pub(crate) fn materialize_builtin_skill(name: &str, root: &Path) -> MietteResult<PathBuf> {
    let dir = BUILTIN_SKILLS.get_dir(name).ok_or_else(|| unknown_skill_error(name))?;

    let out_root = root.join(name);
    fs::create_dir_all(&out_root)
        .map_err(|err| miette!("failed to create '{}': {err}", out_root.display()))?;

    // Entries carry paths relative to the embedded root
    // (`rhei-plan-writer/references/default-states.md`), so stripping the
    // skill's own segment lands them at `out_root`.
    let mut files = Vec::new();
    collect_embedded_skill_files(dir, &mut files);
    for file in files {
        let relative = file
            .path()
            .strip_prefix(name)
            .map_err(|_| miette!("built-in skill '{name}' has an unexpected entry"))?;
        let out = out_root.join(relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| miette!("failed to create '{}': {err}", parent.display()))?;
        }
        fs::write(&out, file.contents())
            .map_err(|err| miette!("failed to write '{}': {err}", out.display()))?;
    }

    Ok(out_root)
}

/// Error for a `--skills` name no source can satisfy, naming what does exist.
/// §FS-rhei-install-skills.4.3
pub(crate) fn unknown_skill_error(name: &str) -> miette::Report {
    miette!(
        "no skill named '{name}'. This binary carries: {}.",
        builtin_skill_names().join(", ")
    )
}

/// Every file under `dir`, at any depth. A skill may nest supporting material
/// under `references/`, which may itself contain subdirectories.
fn collect_embedded_skill_files<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a include_dir::File<'a>>) {
    out.extend(dir.files());
    for nested in dir.dirs() {
        collect_embedded_skill_files(nested, out);
    }
}
