//! Host repair: journal permissions, retire duplicate aliases, revoke sandbox ACLs.
use crate::Result;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    process::{Command, Stdio},
};
pub fn repair(worker: &str) -> Result<Value> {
    let store = super::super::state::Store::open_wait(worker)?;
    let state = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=source_access_repair")?;
    let archive = store
        .directory
        .join("source-access-repair")
        .join(crate::relay_store::nonce());
    kah_core::run_store::ensure_private_directory(&archive)?;
    capture_acl(&store.directory, &archive.join("before.acl"))?;
    let mut before = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(archive.join("modes.jsonl"))?;
    let mut after = vec![];
    let uid = kah_sandbox::current_uid()?;
    inspect(&store.directory, uid, &mut before, &mut after)?;
    before.sync_all()?;
    let retired = super::retire_shared_sources(&store.directory, &state.observer.workspace)?;
    kah_run::access::revoke_tree(&store.directory, kah_run::access::service_uid()?)?;
    for (path, mode) in &after {
        fs::set_permissions(path, fs::Permissions::from_mode(*mode))?;
    }
    capture_acl(&store.directory, &archive.join("after.acl"))?;
    let result = json!({"worker":worker,"retirement":retired,"restored_owned_paths":after.len(),"manifest":archive,"copied_cas_bytes":0});
    crate::unattended_report::save(&archive, "result.json", &result)?;
    Ok(result)
}
fn capture_acl(root: &Path, path: &Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    let status = Command::new("/usr/bin/getfacl")
        .args(["-R", "-P", "-p", "-n"])
        .arg(root)
        .stdout(Stdio::from(file))
        .status()?;
    if !status.success() {
        return Err("REVIEW_SOURCE_ACL_CAPTURE stage=source_access_repair".into());
    }
    Ok(())
}
fn inspect(
    root: &Path,
    uid: u32,
    record: &mut fs::File,
    after: &mut Vec<(std::path::PathBuf, u32)>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            inspect(&path, uid, record, after)?;
        }
        if meta.uid() == uid && (meta.is_dir() || meta.is_file()) {
            let mode = meta.permissions().mode() & 0o700;
            writeln!(
                record,
                "{}",
                json!({"path":path,"before_mode":meta.permissions().mode()&0o7777,"restored_mode":mode,"dev":meta.dev(),"inode":meta.ino()})
            )?;
            after.push((path, mode));
        }
    }
    Ok(())
}
