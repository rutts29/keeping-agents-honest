//! Under Option1 a promoted steer reaches the observer through its rule set and
//! its review requests, not through the legacy history projection. These read
//! both from the stores kah wrote.
use super::*;
use std::time::SystemTime;

/// The steer text as stored in JSON strings (it has nothing to escape).
fn needle(decision: &str) -> &str {
    &decision[..40]
}

/// File name of the current rule set version when its input carries the decision.
pub(super) fn rule_set_with(id: &str, project: &Path, decision: &str) -> Option<String> {
    let base = RunStore::default_state_root().ok()?;
    let run = kah_observer::session::Store::read(&base, id)
        .ok()??
        .run_id?;
    let directory = RunStore::path_for(&base, project, run).ok()?;
    let head: Value =
        serde_json::from_slice(&fs::read(directory.join("current-rules.json")).ok()?).ok()?;
    let name = format!("{}.json", head["digest"].as_str()?.strip_prefix("sha256:")?);
    fs::read_to_string(directory.join("current-rules").join(&name))
        .ok()?
        .contains(needle(decision))
        .then_some(name)
}

/// Observer review request files that mention the decision, with their write time.
pub(super) fn requests_with(id: &str, decision: &str) -> Vec<(String, SystemTime)> {
    fn walk(dir: &Path, depth: u8, decision: &str, found: &mut Vec<(String, SystemTime)>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() && depth > 0 {
                walk(&path, depth - 1, decision, found);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.starts_with("review-")
                && fs::read_to_string(&path).is_ok_and(|t| t.contains(needle(decision)))
                && let Ok(written) = entry.metadata().and_then(|m| m.modified())
            {
                found.push((name.to_owned(), written));
            }
        }
    }
    let mut found = vec![];
    if let Some(state) = kah_cli::observer_review::state::Store::read(id)
        .ok()
        .flatten()
    {
        walk(&state.observer.workspace, 3, decision, &mut found);
    }
    found
}

/// A request written after `since` that carries the decision.
pub(super) fn request_after(id: &str, decision: &str, since: SystemTime) -> Option<String> {
    requests_with(id, decision)
        .into_iter()
        .find(|(_, written)| *written > since)
        .map(|(name, _)| name)
}

/// For timeout reports: every request mentioning the decision and its age.
pub(super) fn request_ages(id: &str, decision: &str) -> Value {
    json!(
        requests_with(id, decision)
            .into_iter()
            .map(|(name, written)| json!({"file":name,"ageSeconds":written.elapsed().ok().map(|a| a.as_secs())}))
            .collect::<Vec<_>>()
    )
}
