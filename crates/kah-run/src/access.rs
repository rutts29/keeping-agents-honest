//! Named ACLs share only disposable run data with rootless Docker's owner.
use std::{fs, path::Path, process::Command};

/// Call before starting the executor. Symlinks in the checkout are never followed.
pub fn share_tree(root: &Path, service_uid: u32, writable: bool) -> Result<(), String> {
    if !fs::symlink_metadata(root).map_err(io_error)?.is_dir() {
        return Err("RUN_SHARE_ROOT stage=worker_prepare: expected real directory".into());
    }
    let operator = kah_sandbox::current_uid().map_err(|e| e.to_string())?;
    let access = if writable { "rwX" } else { "r-X" };
    acl(root, &format!("u:{service_uid}:{access}"), true)?;
    if writable {
        inherit(
            root,
            &format!("d:u:{operator}:rwx,d:u:{service_uid}:rwx,d:o::---"),
        )?;
    }
    Ok(())
}

/// Only traversal is needed above bind mounts; private state remains elsewhere.
pub fn traverse(root: &Path, service_uid: u32) -> Result<(), String> {
    if !fs::symlink_metadata(root).map_err(io_error)?.is_dir() {
        return Err("RUN_SHARE_ROOT stage=worker_prepare: expected real directory".into());
    }
    acl(root, &format!("u:{service_uid}:--x"), false)
}
fn inherit(directory: &Path, entries: &str) -> Result<(), String> {
    acl(directory, entries, false)?;
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_dir() {
            inherit(&entry.path(), entries)?;
        }
    }
    Ok(())
}
fn acl(path: &Path, entries: &str, recursive: bool) -> Result<(), String> {
    let mut command = Command::new("/usr/bin/setfacl");
    command.arg("-P");
    if recursive {
        command.arg("-R");
    }
    let result = command
        .args(["-m", entries, "--"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !result.status.success() {
        return Err(format!(
            "RUN_SHARE_ACL stage=worker_prepare: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(())
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_SHARE_IO stage=worker_prepare: {error}")
}
