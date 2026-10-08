//! git/git v2.53.0 Documentation/glossary-content.adoc pathspec glob,
//! git-diff.adoc, git-ls-tree.adoc and git-log.adoc. Exact committed endpoints.
use crate::{Result, observer_review::artifact_git};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
pub(super) fn sha(value: &str) -> Result<()> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("REVIEW_MAP_SHA stage=component_map".into());
    }
    Ok(())
}
pub(super) fn empty(root: &Path) -> Result<String> {
    Ok(String::from_utf8(artifact_git::git(
        root,
        &["hash-object", "-t", "tree", "/dev/null"],
        None,
    )?)?
    .trim()
    .into())
}
pub(super) fn paths(
    root: &Path,
    base: &str,
    head: &str,
    globs: &[String],
) -> Result<BTreeSet<String>> {
    let mut args = vec![
        "diff".into(),
        "--name-only".into(),
        "-z".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        base.into(),
        head.into(),
        "--".into(),
    ];
    args.extend(globs.iter().map(|pattern| format!(":(top,glob){pattern}")));
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let bytes = artifact_git::git(root, &args, None)?;
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            String::from_utf8(p.to_vec())
                .map_err(|_| "REVIEW_MAP_PATH_ENCODING stage=component_map".into())
        })
        .collect()
}
pub(super) fn inventory(root: &Path, head: &str) -> Result<BTreeMap<String, String>> {
    let bytes = artifact_git::git(root, &["ls-tree", "-rz", "--full-tree", head], None)?;
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let row = std::str::from_utf8(p)
                .map_err(|_| "REVIEW_MAP_PATH_ENCODING stage=component_map")?;
            let (fields, path) = row
                .split_once('\t')
                .ok_or("REVIEW_MAP_TREE stage=component_map")?;
            let object = fields
                .split_whitespace()
                .nth(2)
                .ok_or("REVIEW_MAP_TREE stage=component_map")?;
            Ok((path.to_owned(), object.to_owned()))
        })
        .collect()
}
pub(super) fn history(root: &Path, head: &str, baseline: Option<&str>) -> Result<String> {
    let range = baseline
        .map(|base| format!("{base}..{head}"))
        .unwrap_or_else(|| head.into());
    Ok(String::from_utf8(artifact_git::git(
        root,
        &[
            "log",
            "--diff-merges=first-parent",
            "--root",
            "--format=commit:%H",
            "--name-only",
            "-z",
            &range,
            "--",
        ],
        None,
    )?)?)
}
