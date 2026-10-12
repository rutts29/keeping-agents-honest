//! A verifier bind needs traversal, never listing/read access to its ancestors.
use crate::Result;
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};

pub(super) fn prepare(bind: &Path, uid: u32) -> Result<()> {
    let owner = kah_sandbox::current_uid()?;
    // Same owned-ancestor boundary as native_setup::share. No siblings touched.
    for parent in bind.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent).map_err(|e| {
            format!("RUN_VERIFIER_INFRA_GAP stage=verification: parent metadata: {e}")
        })?;
        if !metadata.is_dir() {
            return Err(
                "RUN_VERIFIER_INFRA_GAP stage=verification: non-directory or symlink ancestor"
                    .into(),
            );
        }
        if metadata.uid() != owner {
            break;
        }
        traverse(parent, uid)
            .map_err(|e| format!("RUN_VERIFIER_INFRA_GAP stage=verification: {e}"))?;
    }
    Ok(())
}

fn traverse(parent: &Path, uid: u32) -> Result<()> {
    let out = Command::new("/usr/bin/getfacl")
        .args(["-cpn"])
        .arg(parent)
        .output()?;
    if !out.status.success() {
        return Err("RUN_VERIFIER_INFRA_GAP stage=verification: ancestor ACL unavailable".into());
    }
    let text = String::from_utf8(out.stdout)?;
    let key = format!("user:{uid}:");
    let prior = text
        .lines()
        .find_map(|line| line.strip_prefix(&key))
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or("---");
    let mask = text
        .lines()
        .find_map(|line| line.strip_prefix("mask::"))
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or("rwx");
    if prior.contains('x') && mask.contains('x') {
        return Ok(());
    }
    // Add traversal without removing an existing named grant or default ACL.
    let permissions = format!(
        "{}{}x",
        if prior.contains('r') { 'r' } else { '-' },
        if prior.contains('w') { 'w' } else { '-' }
    );
    let result = Command::new("/usr/bin/setfacl")
        .args(["-P", "-m", &format!("u:{uid}:{permissions}"), "--"])
        .arg(parent)
        .output()?;
    if !result.status.success() {
        return Err(
            "RUN_VERIFIER_INFRA_GAP stage=verification: ancestor traverse grant failed".into(),
        );
    }
    Ok(())
}
