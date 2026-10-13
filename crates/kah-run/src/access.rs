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
    let inherited = if writable { "rwx" } else { "r-x" };
    owned(
        root,
        &format!("d:u:{operator}:rwx,d:u:{service_uid}:{inherited},d:o::---"),
        operator,
        true,
    )?;
    Ok(())
}

/// Only traversal is needed above bind mounts; private state remains elsewhere.
pub fn traverse(root: &Path, service_uid: u32) -> Result<(), String> {
    if !fs::symlink_metadata(root).map_err(io_error)?.is_dir() {
        return Err("RUN_SHARE_ROOT stage=worker_prepare: expected real directory".into());
    }
    grant(root, service_uid, 'x')
}

/// Grant read on one existing file without rewriting any other entry's rights.
pub fn read_file(path: &Path, service_uid: u32) -> Result<(), String> {
    grant(path, service_uid, 'r')
}
// Native programs replace files under a different mapped UID. Their inherited
// ACL already grants operator access; only owners may edit the ACL itself.
fn owned(root: &Path, entries: &str, operator: u32, directories_only: bool) -> Result<(), String> {
    operate(root, entries, operator, directories_only, "-m")
}
/// Revoke only the sandbox account's named access/default ACLs on owned data.
pub fn revoke_tree(root: &Path, service_uid: u32) -> Result<(), String> {
    let operator = kah_sandbox::current_uid().map_err(|e| e.to_string())?;
    operate(root, &format!("u:{service_uid}"), operator, false, "-x")?;
    operate(root, &format!("d:u:{service_uid}"), operator, true, "-x")
}
fn operate(
    root: &Path,
    entries: &str,
    operator: u32,
    directories_only: bool,
    action: &str,
) -> Result<(), String> {
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
            action,
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
// Add one permission bit to a named user. The shared mask gets only that bit
// on top of its prior value; it is never recalculated from other entries.
fn grant(path: &Path, service_uid: u32, bit: char) -> Result<(), String> {
    let text = getfacl(path)?;
    let (user, mask) = (
        field(&text, &format!("user:{service_uid}:")).unwrap_or("---"),
        field(&text, "mask::").or_else(|| field(&text, "group::")),
    );
    let mask = mask.ok_or_else(|| acl_error("ACL has no mask or group entry".into()))?;
    if user.contains(bit) && mask.contains(bit) {
        return Ok(());
    }
    let with_bit = |bits: &str| -> String {
        "rwx"
            .chars()
            .map(|c| if c == bit || bits.contains(c) { c } else { '-' })
            .collect()
    };
    let entries = format!("u:{service_uid}:{},m::{}", with_bit(user), with_bit(mask));
    let result = Command::new("/usr/bin/setfacl")
        .args(["-P", "-n", "-m", &entries, "--"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !result.status.success() {
        return Err(acl_error(String::from_utf8_lossy(&result.stderr).into()));
    }
    Ok(())
}
fn getfacl(path: &Path) -> Result<String, String> {
    let out = Command::new("/usr/bin/getfacl")
        .args(["-cpn", "--"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !out.status.success() {
        return Err(acl_error(String::from_utf8_lossy(&out.stderr).into()));
    }
    String::from_utf8(out.stdout).map_err(|e| acl_error(e.to_string()))
}
fn field<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|rest| rest.split_whitespace().next())
}
fn acl_error(detail: String) -> String {
    format!("RUN_SHARE_ACL stage=worker_prepare: {detail}")
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
