//! Only tags advertised by the retained object-only bundle enter gate checks.
use super::*;

pub(super) fn import(candidate: &Worktree, bundle: &Path) -> Result<()> {
    if !fs::symlink_metadata(bundle)?.is_file() {
        return Err("REVIEW_GATE_TAG_BUNDLE_TYPE stage=verification".into());
    }
    let output = git().args(["bundle", "list-heads"]).arg(bundle).output()?;
    if !output.status.success() {
        return Err("REVIEW_GATE_TAG_BUNDLE stage=verification".into());
    }
    for line in String::from_utf8(output.stdout)?.lines() {
        let Some((oid, name)) = line.split_once(' ') else {
            return Err("REVIEW_GATE_TAG_REF stage=verification".into());
        };
        if !name.starts_with("refs/tags/") {
            continue;
        }
        if !super::super::super::oid(oid)
            || !git().args(["check-ref-format", name]).status()?.success()
        {
            return Err("REVIEW_GATE_TAG_REF stage=verification".into());
        }
        import_one(candidate, bundle, oid, name)?;
    }
    Ok(())
}

fn import_one(candidate: &Worktree, bundle: &Path, oid: &str, name: &str) -> Result<()> {
    // File transport is allowed only for this checked regular bundle path.
    // All other transports stay disabled; the worker repo is never fetched.
    if !git()
        .arg("-C")
        .arg(&candidate.path)
        .args([
            "-c",
            "protocol.file.allow=always",
            "-c",
            "fetch.fsckObjects=true",
            "-c",
            "transfer.fsckObjects=true",
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
        ])
        .arg(bundle)
        .arg(format!("+{name}:{name}"))
        .status()?
        .success()
    {
        return Err("REVIEW_GATE_TAG_IMPORT stage=verification: tag target unavailable or invalid in retained bundle".into());
    }
    let imported = git()
        .arg("-C")
        .arg(&candidate.path)
        .args(["rev-parse", "--verify", name])
        .output()?;
    if !imported.status.success() || String::from_utf8(imported.stdout)?.trim() != oid {
        return Err("REVIEW_GATE_TAG_BINDING stage=verification".into());
    }
    Ok(())
}
