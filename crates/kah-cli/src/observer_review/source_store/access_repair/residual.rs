//! One-time repair of old input aliases and sandbox-owned archived reports.
use crate::Result;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

pub fn retire_inputs(private: &Path, workspace: &Path) -> Result<Value> {
    let mut originals = BTreeMap::new();
    for path in files(private)? {
        let meta = fs::metadata(&path)?;
        originals.insert((meta.dev(), meta.ino()), path);
    }
    let archive = private.join("source-link-retirement");
    kah_core::run_store::ensure_private_directory(&archive)?;
    let manifest = archive.join(format!("inputs-{}.jsonl", crate::relay_store::nonce()));
    let mut record = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&manifest)?;
    let mut retired = 0;
    for alias in files(workspace)? {
        let meta = fs::metadata(&alias)?;
        let Some(original) = originals.get(&(meta.dev(), meta.ino())) else {
            continue;
        };
        let digest = super::super::retirement::digest(original)?;
        if super::super::retirement::digest(&alias)? != digest {
            return Err("REVIEW_SOURCE_ALIAS_DIGEST stage=source_access_repair".into());
        }
        writeln!(
            record,
            "{}",
            json!({"public":alias,"private":original,"digest":digest,"bytes":meta.len(),"state":"verified_before_unlink"})
        )?;
        record.sync_all()?;
        fs::remove_file(&alias)?;
        if super::super::retirement::digest(original)? != digest {
            return Err("REVIEW_SOURCE_SURVIVOR_DIGEST stage=source_access_repair".into());
        }
        writeln!(
            record,
            "{}",
            json!({"private":original,"digest":digest,"state":"private_survivor_verified"})
        )?;
        retired += 1;
    }
    record.sync_all()?;
    Ok(json!({"retired":retired,"copied_bytes":0,"manifest":manifest}))
}

pub(super) fn privatize_foreign(private: &Path, manifest: &Path) -> Result<Value> {
    let owner = kah_sandbox::current_uid()?;
    let mut record = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(manifest)?;
    let mut count = 0;
    let mut bytes = 0;
    for path in files(private)? {
        let meta = fs::metadata(&path)?;
        if meta.uid() == owner {
            continue;
        }
        let digest = super::super::retirement::digest(&path)?;
        writeln!(
            record,
            "{}",
            json!({"path":path,"previous_uid":meta.uid(),"digest":digest,"bytes":meta.len(),"state":"before_private_copy"})
        )?;
        record.sync_all()?;
        private_copy(&path, &path)?;
        writeln!(
            record,
            "{}",
            json!({"path":path,"digest":digest,"state":"owner_private_copy_verified"})
        )?;
        count += 1;
        bytes += meta.len();
    }
    record.sync_all()?;
    Ok(json!({"files":count,"bytes":bytes,"manifest":manifest}))
}

/// Import bytes, never the sandbox inode, ownership or inherited ACLs.
pub fn private_copy(source: &Path, target: &Path) -> Result<()> {
    copy_inner(source, target)
        .map_err(|error| format!("REVIEW_PRIVATE_COPY stage=source_access_repair: {error}").into())
}

pub fn archive(source: &Path, target: &Path) -> Result<()> {
    private_copy(source, target)?;
    fs::remove_file(source).map_err(|error| {
        format!("REVIEW_PRIVATE_ARCHIVE stage=source_access_repair: {error}").into()
    })
}

fn copy_inner(source: &Path, target: &Path) -> Result<()> {
    kah_core::run_store::ensure_save_directory(
        target
            .parent()
            .ok_or("REVIEW_PRIVATE_COPY_PATH stage=source_access_repair")?,
    )?;
    if !fs::symlink_metadata(source)?.is_file() {
        return Err("REVIEW_PRIVATE_COPY_TYPE stage=source_access_repair".into());
    }
    let digest = super::super::retirement::digest(source)?;
    let temp = target.with_extension(format!("{}.tmp", crate::relay_store::nonce()));
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    let status = Command::new("/usr/bin/setfacl")
        .args(["-b", "--"])
        .arg(&temp)
        .status()?;
    if !status.success() {
        return Err("REVIEW_PRIVATE_COPY_ACL stage=source_access_repair".into());
    }
    fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    if super::super::retirement::digest(&temp)? != digest {
        return Err("REVIEW_PRIVATE_COPY_DIGEST stage=source_access_repair".into());
    }
    fs::rename(&temp, target)?;
    fs::File::open(target.parent().unwrap())?.sync_all()?;
    Ok(())
}

fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut result = vec![];
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if meta.is_dir() {
            result.extend(files(&path)?);
        } else if meta.is_file() {
            result.push(path);
        }
    }
    Ok(result)
}
