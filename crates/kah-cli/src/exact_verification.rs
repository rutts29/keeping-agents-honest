//! Observer follow-ups reuse the run's fresh-checkout and gVisor verifier.
use crate::{Result, native_child::stop_executors};
use kah_run::{access, verification, worktree::Worktree};
use kah_sandbox::{Domain, LaunchOptions, Project};
use std::{path::Path, time::Duration};
mod access_path;
/// #878 (safetensors) in gVisor, cold registry: 2m33s build, 171 s in total.
const DEADLINE: Duration = Duration::from_secs(600);
#[cfg(test)]
pub(crate) fn prepare_access(path: &Path, uid: u32) -> Result<serde_json::Value> {
    access_path::prepare(path, uid)
}

pub fn execute(
    home: &Path,
    candidate: &Worktree,
    commit: &str,
    options: LaunchOptions,
    script: &str,
    log: &Path,
    interrupted: impl FnMut() -> std::result::Result<bool, String>,
) -> Result<(verification::Receipt, bool)> {
    let tree = candidate.verification_snapshot(
        &candidate
            .path
            .parent()
            .unwrap()
            .join(format!("verify-{}", crate::relay_store::nonce())),
        commit,
    )?;
    let result = run(home, &tree, commit, options, script, log, interrupted);
    let cleanup = stop_executors(&tree.path);
    let changed = tree
        .head()
        .and_then(|head| tree.dirty().map(|dirty| head != commit || dirty));
    let recorded = record(log, &result, &changed);
    let removed = tree.remove_verification();
    cleanup?;
    removed?;
    recorded?;
    Ok((result?, changed?))
}
fn run(
    home: &Path,
    tree: &Worktree,
    commit: &str,
    options: LaunchOptions,
    script: &str,
    log: &Path,
    interrupted: impl FnMut() -> std::result::Result<bool, String>,
) -> Result<verification::Receipt> {
    let uid = access::service_uid()?;
    let acl_audit = access_path::prepare(&tree.path, uid)?;
    let acl_path = log.with_extension("ancestor-acl.json");
    crate::unattended_report::save(
        acl_path.parent().unwrap(),
        acl_path.file_name().unwrap().to_str().unwrap(),
        &acl_audit,
    )?;
    access::share_tree(&tree.path, uid, true)?;
    let project = Project {
        domain: Domain::Research,
        root: tree.path.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let executor = crate::native_setup::executor(home, &project, options, Some(&tree.git_dir))?
        .with_read_only_mounts(&[
            (tree.git_dir.clone(), tree.git_dir.clone()),
            (tree.path.join(".git"), tree.path.join(".git")),
        ])?;
    // This is not a review deadline. Review progress/liveness remains
    // independent of test execution.
    let receipt =
        verification::execute_checked(&executor, script, commit, DEADLINE, log, interrupted)?;
    checked_receipt(receipt, log)
}

fn checked_receipt(receipt: verification::Receipt, log: &Path) -> Result<verification::Receipt> {
    if receipt.exit_code == Some(126)
        && receipt.output.contains("invalid mount config")
        && receipt.output.contains("permission denied")
    {
        return Err(format!(
            "RUN_VERIFIER_INFRA_GAP stage=verification: bind traversal denied; retained log {}",
            log.display()
        )
        .into());
    }
    Ok(receipt)
}

pub(crate) fn receipt_path(log: &Path) -> std::path::PathBuf {
    log.with_extension("receipt.json")
}

fn record(
    log: &Path,
    result: &Result<verification::Receipt>,
    changed: &std::result::Result<bool, String>,
) -> Result<()> {
    if let (Ok(receipt), Ok(changed)) = (result, changed) {
        let path = receipt_path(log);
        crate::unattended_report::save(
            path.parent()
                .ok_or("RUN_TEST_RECEIPT_PATH stage=verification")?,
            path.file_name().unwrap().to_str().unwrap(),
            &serde_json::json!({"receipt":receipt,"changed":changed}),
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/observer_unit/verifier_access.rs"]
mod tests;
