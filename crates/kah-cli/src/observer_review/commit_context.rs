//! Native git/git v2.53.0 Documentation/git-diff.adoc: compare exact endpoints.
//! Executed by the existing context collector, inside gVisor for restricted work.
use crate::Result;
use serde_json::{Value, json};
use std::{path::Path, process::Command};
mod tree;
pub use tree::collect as tree;
pub fn collect(root: &Path, baseline: Option<&str>) -> Result<Value> {
    let head = text(root, &["rev-parse", "--verify", "HEAD"])?;
    exact(root, baseline, head.trim())
}
pub fn exact(root: &Path, baseline: Option<&str>, head: &str) -> Result<Value> {
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("REVIEW_COMMIT_HEAD stage=context".into());
    }
    let base = if let Some(base) = baseline {
        if !matches!(base.len(), 40 | 64) || !base.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("REVIEW_COMMIT_BASELINE stage=context".into());
        }
        base.to_owned()
    } else {
        text(root, &["hash-object", "-t", "tree", "/dev/null"])?
            .trim()
            .to_owned()
    };
    let diff = text(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--binary",
            &base,
            head,
            "--",
        ],
    )?;
    let names = bytes(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            &base,
            head,
            "--",
        ],
    )?;
    let mut files = vec![];
    for raw in names.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let mut value = file(root, head, raw)?;
        let path = value["path"].as_str().unwrap();
        value["patch"] = json!(text(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--binary",
                &base,
                head,
                "--",
                path
            ]
        )?);
        files.push(value);
    }
    Ok(
        json!({"baseline":baseline,"baseline_tree":base,"head":head,"diff":diff,"touched_files":files,"whole_files":true,"content_cut":false}),
    )
}
fn file(root: &Path, head: &str, raw: &[u8]) -> Result<Value> {
    let path=std::str::from_utf8(raw).map_err(|_|"REVIEW_COMMIT_PATH_ENCODING stage=context: original Git diff remains available; native non-UTF8 path needs explicit handling")?;
    let spec = format!("{head}:{path}");
    if bytes(root, &["ls-tree", "-z", head, "--", path])?.is_empty() {
        return Ok(json!({"path":path,"deleted":true}));
    }
    let contents = bytes(root, &["show", &spec])?;
    let blob = text(root, &["rev-parse", &spec])?;
    let content = match String::from_utf8(contents) {
        Ok(value) => json!({"encoding":"utf8","text":value}),
        Err(error) => json!({"encoding":"bytes","bytes":error.into_bytes()}),
    };
    Ok(json!({"path":path,"blob":blob.trim(),"content":content,"deleted":false}))
}
fn text(root: &Path, args: &[&str]) -> Result<String> {
    String::from_utf8(bytes(root, args)?)
        .map_err(|_| "REVIEW_COMMIT_TEXT_ENCODING stage=context".into())
}
fn bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = output(root, args)?;
    if !output.status.success() {
        return Err(format!(
            "REVIEW_COMMIT_GIT stage=context: git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}
/// git/git v2.53.0 Documentation/git-merge-base.adoc: 0=ancestor,1=not.
pub fn ancestor(root: &Path, from: &str, to: &str) -> Result<bool> {
    if [from, to]
        .iter()
        .any(|h| !matches!(h.len(), 40 | 64) || !h.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("REVIEW_COMMIT_HEAD stage=context".into());
    }
    match output(root, &["merge-base", "--is-ancestor", from, to])?
        .status
        .code()
    {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("REVIEW_COMMIT_ANCESTRY stage=context".into()),
    }
}
fn output(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Ok(Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
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
        .current_dir(root)
        .output()?)
}
