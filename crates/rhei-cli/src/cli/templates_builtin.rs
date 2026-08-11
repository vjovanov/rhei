// Templates compiled into the binary, so `rhei templates` is populated on a
// bare `cargo install` with no companion asset directory to install alongside
// it. §FS-rhei-templates.1

use include_dir::{include_dir, Dir};

/// The shipped template library, embedded at compile time.
///
/// Living under the CLI package (rather than the repo's `.agents/`) is what
/// makes these files part of the published crate: `cargo install` extracts the
/// package, and a directory outside it would simply not be there.
static BUILTIN_TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Names of the built-in templates, in listing order.
pub(super) fn builtin_template_names() -> Vec<String> {
    let mut names: Vec<String> = BUILTIN_TEMPLATES
        .dirs()
        .filter_map(|dir| dir.path().file_name()?.to_str().map(ToOwned::to_owned))
        .collect();
    names.sort();
    names
}

/// Whether a built-in template of this name exists.
pub(super) fn builtin_template_exists(name: &str) -> bool {
    BUILTIN_TEMPLATES.get_dir(name).is_some()
}

/// A built-in template extracted to disk. `path()` is the template directory —
/// named after the template, because manifest validation checks that the
/// directory name and the manifest `name` agree.
pub(super) struct ExtractedTemplate {
    _root: tempfile::TempDir,
    path: PathBuf,
}

impl ExtractedTemplate {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

/// Extract a built-in template into a temporary directory and return it.
///
/// The instantiation pipeline reads templates from the filesystem — manifest,
/// plan skeleton, states, settings, task files. Rather than teach every step to
/// read from two kinds of source, a built-in is materialized once and then
/// behaves exactly like a project or user template. The returned handle owns
/// the extraction: it must outlive the use of the path.
pub(super) fn materialize_builtin_template(name: &str) -> MietteResult<ExtractedTemplate> {
    let dir = BUILTIN_TEMPLATES
        .get_dir(name)
        .ok_or_else(|| {
            miette!(
                help = did_you_mean(name, &builtin_template_names())
                    .unwrap_or_else(|| "this binary carries no templates.".to_string()),
                "no built-in template named '{name}'"
            )
        })?;

    let temp = tempfile::Builder::new()
        .prefix("rhei-builtin-template-")
        .tempdir()
        .map_err(|err| miette!(
help = embedded_extraction_help(),
"failed to create a temporary directory: {err}"))?;
    let root = temp.path().join(name);
    fs::create_dir_all(&root)
        .map_err(|err| miette!(
help = embedded_extraction_help(),
"failed to create '{}': {err}", root.display()))?;

    // Entries carry paths relative to the embedded root (`<name>/tasks/01.md`),
    // so stripping the template's own segment lands them at the temp root.
    let mut files = Vec::new();
    collect_embedded_files(dir, &mut files);
    for file in files {
        let relative = file
            .path()
            .strip_prefix(name)
            .map_err(|_| miette!(
help = internal_error_help(),
"built-in template '{name}' has an unexpected entry"))?;
        let out = root.join(relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| miette!(
help = embedded_extraction_help(),
"failed to create '{}': {err}", parent.display()))?;
        }
        fs::write(&out, file.contents())
            .map_err(|err| miette!(
help = embedded_extraction_help(),
"failed to write '{}': {err}", out.display()))?;
    }

    Ok(ExtractedTemplate { _root: temp, path: root })
}

/// Every file under `dir`, at any depth. A workspace template nests task files
/// under `tasks/`, which may itself contain subdirectories.
fn collect_embedded_files<'a>(dir: &'a Dir<'a>, out: &mut Vec<&'a include_dir::File<'a>>) {
    out.extend(dir.files());
    for nested in dir.dirs() {
        collect_embedded_files(nested, out);
    }
}
