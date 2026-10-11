//! Host-private records; policy changes are explicit data edits, never learning.
use crate::Result;
use kah_observer::commit_review::{DoubtLine, Review, recalibration};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn policy(home: &Path) -> Result<DoubtLine> {
    let directory = home.join(".config/kah");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let path = directory.join("commit-policy.json");
    if !path.exists() {
        let _lock =
            super::super::state::Store::delivery_at(&directory.join("commit-policy"), true)?;
        if !path.exists() {
            let value: Value = serde_json::from_str(include_str!(
                "../../../../../assets/observer/commit-policy.json"
            ))?;
            crate::unattended_report::save(&directory, "commit-policy.json", &value)?;
        }
    }
    let policy: DoubtLine = serde_json::from_slice(&super::super::consume::read_report(&path)?)?;
    policy.validate()?;
    Ok(policy)
}
fn directory(worker: &str) -> Result<PathBuf> {
    let directory = super::super::state::Store::directory(worker)?.join("commit-reviews");
    kah_core::run_store::ensure_private_directory(&directory)?;
    Ok(directory)
}
pub fn save(record: &Review) -> Result<()> {
    if !matches!(record.head.len(), 40 | 64) || !record.head.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("COMMIT_HEAD_INVALID stage=commit_journal".into());
    }
    let root = directory(&record.worker)?;
    let value = serde_json::to_value(record)?;
    let archive = root.join("versions");
    kah_core::run_store::ensure_private_directory(&archive)?;
    let digest = kah_observer::context_selection::digest(&value);
    crate::unattended_report::save(&archive, &format!("{digest}.json"), &value)?;
    crate::unattended_report::save(&root, &format!("{}.json", record.head), &value)
}
pub fn load(worker: &str, head: &str) -> Result<Option<Review>> {
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("COMMIT_HEAD_INVALID stage=commit_journal".into());
    }
    super::super::state::read(&directory(worker)?.join(format!("{head}.json")))
}
pub fn all(worker: &str) -> Result<Vec<Review>> {
    let mut paths = fs::read_dir(directory(worker)?)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            serde_json::from_slice(&super::super::consume::read_report(&p)?).map_err(Into::into)
        })
        .collect()
}
pub fn cli(worker: &str, line: &str) -> Result<()> {
    let report = recalibration::report(&all(worker)?, line.parse()?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
