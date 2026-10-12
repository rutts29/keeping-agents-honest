//! Resolve virtual catalogue paths before reading or choosing the expansion root.
use super::*;
fn digest(path: &Path) -> Option<&str> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()))
}
pub(super) fn path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(digest(&absolute).map_or_else(|| absolute.clone(), |id| resolved(&absolute, id)))
}
pub(super) fn content(path: &Path) -> Result<Vec<u8>> {
    // Existing resolver, verified storage segments and publication permissions;
    // this reader never copies or publishes any extra source into the view.
    if let Some(id) = digest(path) {
        use std::io::Read;
        let mut reader = segments::reader::open(path, id).map_err(|e| error(path, e.as_ref()))?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| error(path, &e))?;
        return Ok(bytes);
    }
    read_bytes(path)
}
fn error(path: &Path, error: &dyn std::error::Error) -> Box<dyn std::error::Error> {
    let message = error.to_string();
    if message.contains(" stage=") {
        message.into()
    } else {
        format!(
            "REVIEW_SOURCE_IO stage=source_store: {}: {message}",
            path.display()
        )
        .into()
    }
}
pub(super) fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| {
        format!(
            "REVIEW_SOURCE_IO stage=source_store: {}: {e}",
            path.display()
        )
        .into()
    })
}
