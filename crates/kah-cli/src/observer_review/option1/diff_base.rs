//! Where a commit or checkpoint diff starts.
use serde_json::Value;
use std::path::Path;

/// The recorded baseline's head. With no baseline file, a kah run worktree
/// starts from the commit its run was created from, never the empty tree
/// (otherwise the first checkpoint would count files already committed in the
/// base, such as AGENTS.md and CLAUDE.md, as new). Interactive sessions are
/// unchanged.
pub fn diff_base(baseline: Option<&Value>, project: &Path) -> Option<String> {
    match baseline {
        Some(baseline) => baseline["head"].as_str().map(str::to_owned),
        None => run_base_commit(project),
    }
}

fn run_base_commit(project: &Path) -> Option<String> {
    let id = project.parent()?.file_name()?.to_str()?;
    if !id.starts_with("run-") {
        return None;
    }
    let runs = kah_core::run_store::RunStore::default_state_root()
        .ok()?
        .join("runs");
    let record = kah_run::store::Store::read(&runs, id).ok()?;
    if record.worktree != project {
        return None;
    }
    let path = kah_run::store::Store::path(&runs, id)
        .ok()?
        .join("worktree.json");
    Some(kah_run::worktree::Worktree::load(&path).ok()?.base_commit)
}
