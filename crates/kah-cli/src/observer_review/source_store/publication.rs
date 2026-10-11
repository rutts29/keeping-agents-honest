//! A read-only, independent source view for one published request.
use crate::Result;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{BufReader, Read},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};
mod catalogue;
mod graph;
mod references;
pub(super) fn share(private: &Path, input: &Path) -> Result<()> {
    if !input.join("context-input.json").is_file() {
        return Ok(());
    }
    let catalogue: Value = serde_json::from_slice(&fs::read(input.join("context-input.json"))?)?;
    let files = catalogue::files(private, input, &catalogue)?;
    share_files(private, input, files)
}
pub(super) fn share_files(private: &Path, input: &Path, files: BTreeSet<PathBuf>) -> Result<()> {
    let public = input.join("sources");
    fs::create_dir_all(&public)?;
    fs::set_permissions(&public, fs::Permissions::from_mode(0o700))?;
    let mut refs = vec![];
    for path in files {
        if path.extension().is_some_and(|v| v == "json") || envelope(&path)? {
            refs.extend(references::collect(BufReader::new(fs::File::open(path)?))?);
        }
    }
    graph::publish(&private.join("sources"), &public, refs)?;
    kah_run::access::share_tree(&public, kah_run::access::service_uid()?, false)?;
    if let Some(workspace) = input.ancestors().find(|p| p.join(".source-root").is_file()) {
        let marker = workspace.join(".source-view-current");
        let temporary = workspace.join(format!("source-view-{}.tmp", crate::relay_store::nonce()));
        symlink(&public, &temporary)?;
        fs::rename(temporary, marker)?;
    }
    Ok(())
}
fn envelope(path: &Path) -> Result<bool> {
    for byte in BufReader::new(fs::File::open(path)?).bytes() {
        let byte = byte?;
        if !byte.is_ascii_whitespace() {
            return Ok(matches!(byte, b'{' | b'['));
        }
    }
    Ok(false)
}
