//! Host facts an observer cannot read from native records: where the worker's
//! commands ran, and which native tool produced Codex file-change records.
use super::state::State;
use serde_json::{Value, json};
use std::path::Path;

/// Attached to every Option1 review job as `execution`. Host-owned evidence,
/// never worker text; the observer decides applicability from it.
pub(crate) fn fact(home: &Path, state: &State) -> Value {
    let domain = domain(home, state);
    let mut fact = json!({
        "source": "kah controller fact (host-owned; not worker text)",
        "domain": domain,
        "commands": commands(&domain),
    });
    if state.worker.provider == "codex" {
        // openai/codex codex-rs/app-server-protocol/src/protocol/item_builders.rs:
        // ThreadItem::FileChange is built only from apply_patch events
        // (ApplyPatchApprovalRequest, PatchApplyBegin, PatchApplyEnd).
        fact["native_tools"] = json!({
            "fileChange": "apply_patch (Codex records file changes only from apply_patch events)"
        });
    }
    fact
}

/// Every review stage carries the fact; the current-rules intake does not judge
/// rows, so it is left unchanged.
pub(crate) fn attach(mut job: Value, fact: Value) -> Value {
    if job["stage"] != "current_rules" && job.get("execution").is_none() {
        job["execution"] = fact;
    }
    job
}

fn domain(home: &Path, state: &State) -> String {
    let project = &state.worker.project;
    if let Some(domain) = run_domain(project) {
        return domain;
    }
    let policy = kah_core::run_store::RunStore::default_state_root()
        .ok()
        .and_then(|base| crate::session_policy::saved(&base, &state.worker.session).ok())
        .flatten()
        .map(|p| p.policy);
    policy
        .as_deref()
        .and_then(policy_domain)
        .or_else(|| path_domain(home, project))
        .unwrap_or("unknown")
        .into()
}

/// `kah run` worktrees: the run record owns the domain.
fn run_domain(project: &Path) -> Option<String> {
    let id = project.parent()?.file_name()?.to_str()?;
    if !id.starts_with("run-") {
        return None;
    }
    let base = kah_core::run_store::RunStore::default_state_root().ok()?;
    let record = kah_run::store::Store::read(&base.join("runs"), id).ok()?;
    (record.worktree == project).then_some(record.domain)
}

/// Saved session policies are named `<domain>-<mode>`, e.g. `research-online`.
pub(crate) fn policy_domain(policy: &str) -> Option<&'static str> {
    known(policy.split('-').next()?)
}

/// Interactive projects live at `~/repos/<domain>/<project>`.
pub(crate) fn path_domain(home: &Path, project: &Path) -> Option<&'static str> {
    known(
        project
            .strip_prefix(home.join("repos"))
            .ok()?
            .components()
            .next()?
            .as_os_str()
            .to_str()?,
    )
}

fn known(name: &str) -> Option<&'static str> {
    ["mine", "research", "untrusted"]
        .into_iter()
        .find(|d| *d == name)
}

pub(crate) fn commands(domain: &str) -> &'static str {
    match domain {
        "mine" => "host shell; no gVisor sandbox",
        "research" | "untrusted" => "gVisor sandbox",
        _ => "unknown",
    }
}

#[cfg(test)]
#[path = "../../tests/observer_unit/execution_fact.rs"]
mod tests;
