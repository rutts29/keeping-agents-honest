//! Retire only proven duplicate public links; preserve every canonical byte.
use crate::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};
pub(super) fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn retire(private: &Path, workspace: &Path) -> Result<serde_json::Value> {
    let public = workspace.join("sources");
    if !public.is_dir() {
        return Ok(json!({"retired":0}));
    }
    fs::set_permissions(&public, fs::Permissions::from_mode(0o700))?;
    let archive = private.join("source-link-retirement");
    kah_core::run_store::ensure_private_directory(&archive)?;
    let record = archive.join(format!("{}.jsonl", crate::relay_store::nonce()));
    let mut log = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&record)?;
    let mut retired = 0;
    let mut retained = 0;
    for entry in fs::read_dir(&public)? {
        let entry = entry?;
        let survivor = private.join("sources").join(entry.file_name());
        let before = entry.metadata()?;
        let Ok(original) = fs::symlink_metadata(&survivor) else {
            retained += 1;
            continue;
        };
        if !original.is_file() || (before.dev(), before.ino()) != (original.dev(), original.ino()) {
            retained += 1;
            continue;
        }
        let hash = digest(&entry.path())?;
        writeln!(
            log,
            "{}",
            json!({"public":entry.path(),"private":survivor,"dev":before.dev(),"inode":before.ino(),"bytes":before.len(),"stored_sha256":hash,"state":"verified_before_unlink"})
        )?;
        log.sync_all()?;
        fs::remove_file(entry.path())?;
        let after = fs::metadata(&survivor)?;
        if (after.dev(), after.ino()) != (before.dev(), before.ino()) || digest(&survivor)? != hash
        {
            return Err("REVIEW_SOURCE_RETIREMENT_DIGEST stage=source_retirement".into());
        }
        writeln!(
            log,
            "{}",
            json!({"private":survivor,"stored_sha256":hash,"state":"private_survivor_verified"})
        )?;
        retired += 1;
    }
    log.sync_all()?;
    fs::File::open(&public)?.sync_all()?;
    if retained == 0 {
        fs::remove_dir(&public)?;
    }
    Ok(
        json!({"retired":retired,"independent_or_missing_private_retained":retained,"manifest":record,"copied_bytes":0}),
    )
}
