//! Existing read-only collector; native Git exact endpoints for commit packets.
use crate::Result;
use serde_json::json;
use std::{fs, path::Path, process::Command};
/// Runs only in the read-only gVisor context collector. No model or credentials.
pub fn cli(baseline: Option<&str>) -> Result<()> {
    let root = std::env::current_dir()?;
    let head = git(&root, &["rev-parse", "--verify", "HEAD"])?;
    let committed = committed_snapshot(&root, head.as_deref())?;
    let artifact_blobs = if let Some(head) = head.as_deref() {
        super::super::artifact_collect::committed(&root, head.trim())?
    } else {
        Default::default()
    };
    let diff = git(
        &root,
        &["diff", "--no-ext-diff", "--no-textconv", "--binary"],
    )?;
    let staged = git(
        &root,
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
        ],
    )?;
    let status = git(
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let tags = git(&root, &["show-ref", "--tags"])?;
    if root.join(".git").exists() && (diff.is_none() || staged.is_none() || status.is_none()) {
        return Err("REVIEW_GIT_INCOMPLETE stage=context: cannot read complete Git context".into());
    }
    let plans = plans(&root)?;
    let mut context = json!({"head":head.map(|h|h.trim().to_owned()),"committed_snapshot":committed,"artifact_blobs":artifact_blobs,"diff":format!("{}\n{}",diff.unwrap_or_default(),staged.unwrap_or_default()),"status":status,"tags":tags,"plans":plans,"full_context_not_truncated":true});
    if let Some(baseline) = baseline {
        context["commit_context"] = super::super::commit_context::collect(
            &root,
            (baseline != "empty").then_some(baseline),
        )?;
        let head = context["head"]
            .as_str()
            .ok_or("REVIEW_COMMIT_HEAD stage=context")?;
        if context["commit_context"]["head"] != head {
            return Err("REVIEW_COMMIT_HEAD_CHANGED stage=context".into());
        }
        context["full_tree_files"] =
            serde_json::to_value(super::super::commit_context::tree(&root, head)?)?;
    }
    println!("{context}");
    Ok(())
}
fn plans(root: &Path) -> Result<Vec<serde_json::Value>> {
    let mut plans = vec![];
    for path in [
        "docs/PLAN.md",
        "docs/STATUS.md",
        "docs/HANDOFF.md",
        "PLAN.md",
        "AGENTS.md",
        "CLAUDE.md",
    ] {
        match fs::read_to_string(root.join(path)) {
            Ok(text) => plans.push(json!({"path":path,"text":text})),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("REVIEW_PLAN_READ stage=context: {path}: {e}").into()),
        }
    }
    Ok(plans)
}
fn committed_snapshot(root: &Path, head: Option<&str>) -> Result<String> {
    let Some(head) = head else {
        return Ok(String::new());
    };
    // Diffing the empty tree captures ALL earlier phases, even on a clean tree.
    // hash-object without -w computes the native SHA-1/SHA-256 empty tree ID
    // without writing an object or authoring a commit.
    let empty = git(root, &["hash-object", "-t", "tree", "/dev/null"])?
        .ok_or("REVIEW_EMPTY_TREE stage=context")?;
    git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            empty.trim(),
            head.trim(),
            "--",
        ],
    )?
    .ok_or_else(|| {
        "REVIEW_COMMITTED_CONTEXT stage=context: cannot read cumulative HEAD snapshot".into()
    })
}
fn git(root: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .current_dir(root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
            "-c",
        ])
        .arg(format!("safe.directory={}", root.display()))
        .args(args)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "REVIEW_GIT_COMMAND stage=context: git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
}
