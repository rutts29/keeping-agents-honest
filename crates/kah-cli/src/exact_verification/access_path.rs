//! A verifier bind needs traversal, never listing/read access to its ancestors.
use crate::Result;
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};

pub(super) fn prepare(bind: &Path, uid: u32) -> Result<serde_json::Value> {
    let owner = kah_sandbox::current_uid()?;
    let mut audit = vec![];
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
        audit.push(
            traverse(parent, uid)
                .map_err(|e| format!("RUN_VERIFIER_INFRA_GAP stage=verification: {e}"))?,
        );
    }
    Ok(serde_json::json!(audit))
}

fn traverse(parent: &Path, uid: u32) -> Result<serde_json::Value> {
    let out = Command::new("/usr/bin/getfacl")
        .args(["-cpn"])
        .arg(parent)
        .output()?;
    if !out.status.success() {
        return Err("RUN_VERIFIER_INFRA_GAP stage=verification: ancestor ACL unavailable".into());
    }
    let text = String::from_utf8(out.stdout)?;
    let key = format!("user:{uid}:");
    let prior = field(&text, &key).unwrap_or("---");
    let mask = field(&text, "mask::").unwrap_or_else(|| field(&text, "group::").unwrap_or("---"));
    let audit =
        serde_json::json!({"path":parent,"service_uid":uid,"prior_acl":text,"prior_mask":mask});
    if prior.contains('x') && mask.contains('x') {
        return Ok(audit);
    }
    // Add traversal without removing an existing named grant or default ACL.
    let permissions = format!(
        "{}{}x",
        if prior.contains('r') { 'r' } else { '-' },
        if prior.contains('w') { 'w' } else { '-' }
    );
    let next_mask = format!(
        "{}{}x",
        if mask.contains('r') { 'r' } else { '-' },
        if mask.contains('w') { 'w' } else { '-' }
    );
    let result = Command::new("/usr/bin/setfacl")
        // Never recalculate the mask from latent permissions in other entries.
        // An absent x requires a shared-mask x; its prior value is audited.
        .args([
            "-P",
            "-n",
            "-m",
            &format!("u:{uid}:{permissions},m::{next_mask}"),
            "--",
        ])
        .arg(parent)
        .output()?;
    if !result.status.success() {
        return Err(
            "RUN_VERIFIER_INFRA_GAP stage=verification: ancestor traverse grant failed".into(),
        );
    }
    Ok(audit)
}

fn field<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|s| s.split_whitespace().next())
}
