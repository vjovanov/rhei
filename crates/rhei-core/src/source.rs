//! Reading a plan, a workspace index, or a project manifest off disk.
//!
//! One function rather than `fs::read_to_string` at each site, because a
//! command may be holding the file's own lock while it asks the loader to read
//! it. Where a lock is mandatory — a Windows byte range belongs to the handle
//! that took it — that read is refused, and only the process holding the lock
//! can answer it. The driver installs a reader that can; a library consumer
//! that takes no locks never notices.

use std::path::Path;

type Reader = fn(&Path) -> std::io::Result<String>;

static READER: std::sync::OnceLock<Reader> = std::sync::OnceLock::new();

/// Install the reader every plan source goes through. First call wins, and a
/// second is ignored rather than fatal: this is a process-wide convenience, not
/// a contract between two callers.
// §FS-rhei-new.4
pub fn set_reader(reader: Reader) {
    let _ = READER.set(reader);
}

/// Read `path`, through the installed reader when there is one.
// §FS-rhei-new.4
pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    match READER.get() {
        Some(reader) => reader(path),
        None => std::fs::read_to_string(path),
    }
}
