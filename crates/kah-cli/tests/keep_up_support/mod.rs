use kah_cli::observer_review::source_store::{Select, read_selected};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
mod cursor;
mod recording;
pub use recording::{Recording, reconstruct};
mod snapshot;
pub use cursor::Cursor;
mod stats;
pub use stats::{Accumulator, summary};
pub fn sample(root: &Path, repo: &Path) -> Value {
    snapshot::read(root, repo)
}
pub fn sample_cursor(root: &Path, repo: &Path, cursor: &mut Cursor) -> Value {
    snapshot::read_cached(root, repo, cursor)
}

fn paths(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    Ok(paths)
}
fn modified(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}
