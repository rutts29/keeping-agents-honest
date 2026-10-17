//! Under Option1 a promoted steer reaches the observer through its rule set and
//! its review requests, not through the legacy history projection. A request
//! carries the rule set by digest and rules by id. These read both from the
//! stores kah wrote.
use super::*;

/// The steer text as stored in JSON strings (it has nothing to escape).
fn needle(decision: &str) -> &str {
    &decision[..40]
}

fn run_directory(id: &str, project: &Path) -> Option<PathBuf> {
    let base = RunStore::default_state_root().ok()?;
    let run = kah_observer::session::Store::read(&base, id)
        .ok()??
        .run_id?;
    RunStore::path_for(&base, project, run).ok()
}

/// Whether the stored rule set version `<hash>.json` carries the decision in its input.
fn version_has(directory: &Path, name: &str, decision: &str) -> bool {
    fs::read_to_string(directory.join("current-rules").join(name))
        .is_ok_and(|text| text.contains(needle(decision)))
}

/// File name of the current (head) rule set version when its input carries the decision.
pub(super) fn rule_set_with(id: &str, project: &Path, decision: &str) -> Option<String> {
    let directory = run_directory(id, project)?;
    let head: Value =
        serde_json::from_slice(&fs::read(directory.join("current-rules.json")).ok()?).ok()?;
    let name = format!("{}.json", head["digest"].as_str()?.strip_prefix("sha256:")?);
    version_has(&directory, &name, decision).then_some(name)
}

/// Every review request kah stored for the worker: (id, created_ms, cited rule set
/// digest, rule ids). A request carries the current rule set by digest and the
/// selected rules by id, not as inline text.
fn requests(id: &str) -> Vec<(String, u64, String, Vec<String>)> {
    let Ok(directory) = kah_cli::observer_review::state::Store::directory(id) else {
        return vec![];
    };
    fs::read_dir(directory.join("requests"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let value: Value = serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
            let input = &value["trigger"]["option1"]["input"];
            Some((
                value["id"].as_str()?.to_owned(),
                value["created_ms"].as_u64()?,
                input["carried"]["source_digest"].as_str()?.to_owned(),
                input["rules"].as_object()?.keys().cloned().collect(),
            ))
        })
        .collect()
}

/// A request created after `after_ms` that cites a rule set version carrying the
/// decision (by digest). A request citing no such set does not count.
pub(super) fn request_citing(
    id: &str,
    project: &Path,
    decision: &str,
    after_ms: u64,
) -> Option<String> {
    let directory = run_directory(id, project)?;
    requests(id)
        .into_iter()
        .filter(|(_, created, cited, _)| {
            *created > after_ms
                && cited
                    .strip_prefix("sha256:")
                    .is_some_and(|hash| version_has(&directory, &format!("{hash}.json"), decision))
        })
        .min_by_key(|(_, created, _, _)| *created)
        .map(|(request, _, cited, rules)| {
            format!("{request} cites {cited}; rules {}", rules.join(","))
        })
}

/// For timeout reports: every request with its age and the rule set it cites.
pub(super) fn request_citations(id: &str) -> Value {
    let now = kah_observer::session::now_ms();
    json!(
        requests(id)
            .into_iter()
            .map(|(request, created, cited, rules)| json!({"request":request,"ageSeconds":now.saturating_sub(created) / 1000,"citesRuleSet":cited,"rules":rules}))
            .collect::<Vec<_>>()
    )
}
