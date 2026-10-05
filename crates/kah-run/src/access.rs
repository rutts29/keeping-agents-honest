//! Named ACLs share only disposable run data with rootless Docker's owner.
use std::{fs, path::Path, process::Command};

/// Call before starting the executor. Symlinks in the checkout are never followed.
pub fn share_tree(root: &Path, service_uid: u32, writable: bool) -> Result<(), String> {
    if !fs::symlink_metadata(root).map_err(io_error)?.is_dir() {
        return Err("RUN_SHARE_ROOT stage=worker_prepare: expected real directory".into());
    }
    let operator = kah_sandbox::current_uid().map_err(|e| e.to_string())?;
    let access = if writable { "rwX" } else { "r-X" };
    owned(root, &format!("u:{service_uid}:{access}"), operator, false)?;
    if writable {
        owned(
            root,
            &format!("d:u:{operator}:rwx,d:u:{service_uid}:rwx,d:o::---"),
            operator,
            true,
        )?;
    }
    Ok(())
}

/// Only traversal is needed above bind mounts; private state remains elsewhere.
pub fn traverse(root: &Path, service_uid: u32) -> Result<(), String> {
    if !fs::symlink_metadata(root).map_err(io_error)?.is_dir() {
        return Err("RUN_SHARE_ROOT stage=worker_prepare: expected real directory".into());
    }
    acl(root, &format!("u:{service_uid}:--x"))
}
// Native programs replace files under a different mapped UID. Their inherited
// ACL already grants operator access; only owners may edit the ACL itself.
fn owned(root: &Path, entries: &str, operator: u32, directories_only: bool) -> Result<(), String> {
    let mut command = Command::new("/usr/bin/find");
    command
        .arg("-P")
        .arg(root)
        // Native plugin caches remove lock files during traversal. GNU find
        // tolerates only that readdir/stat race, not access or ACL failures.
        .args(["-ignore_readdir_race", "-uid", &operator.to_string()]);
    if directories_only {
        command.args(["-type", "d"]);
    }
    let result = command
        .args([
            "-execdir",
            "/usr/bin/setfacl",
            "-P",
            "-m",
            entries,
            "--",
            "{}",
            "+",
        ])
        .env("PATH", "/usr/bin:/bin")
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
fn acl(path: &Path, entries: &str) -> Result<(), String> {
    let mut command = Command::new("/usr/bin/setfacl");
    command.arg("-P");
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

pub fn service_uid() -> Result<u32, String> {
    let output = Command::new("/usr/bin/id")
        .args(["-u", "sbxrun"])
        .output()
        .map_err(io_error)?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| "RUN_SERVICE_ACCOUNT stage=worker_prepare".into())
}
