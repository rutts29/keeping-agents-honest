use super::{fixture::save, journey::Worker};
use kah_cli::observer_review::{self as review, state::Store};
use serde_json::{Value, json};
use std::{fs, io::BufReader};

pub fn result_ids(worker: &str) -> Vec<String> {
    let root = Store::directory(worker).unwrap().join("results");
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().is_some_and(|e| e == "json"))
                .then(|| path.file_stem().unwrap().to_str().unwrap().to_owned())
        })
        .collect()
}
pub fn result(worker: &str, id: &str) -> Value {
    review::state::read(
        &Store::directory(worker)
            .unwrap()
            .join("results")
            .join(format!("{id}.json")),
    )
    .unwrap()
    .unwrap()
}
pub fn completed_ms(worker: &str, result: &Value) -> u64 {
    let id = result["request"].as_str().unwrap();
    let timing: Value = review::state::read(
        &Store::directory(worker)
            .unwrap()
            .join("stage-timings")
            .join(format!("{id}.json")),
    )
    .unwrap()
    .unwrap();
    timing["at_ms"]["completed"].as_u64().unwrap_or_else(|| {
        let request = review::workspace::request(&Store::open_wait(worker).unwrap(), id).unwrap();
        request.created_ms + result["elapsed_ms"].as_u64().unwrap()
    })
}
pub fn retain_wake(worker: &Worker, result: &Value, phase: usize) {
    let state = Store::read(&worker.id).unwrap().unwrap();
    super::journey::register_session(&worker.root, state.observer.session.as_deref().unwrap());
    let transcript = state
        .observer
        .transcript
        .as_ref()
        .expect("TRACK2_TRANSCRIPT stage=usage");
    let usage = native_usage(transcript, &worker.provider);
    let path = worker
        .root
        .join(format!("{}-usage-last.json", worker.provider));
    let previous = fs::read(&path)
        .ok()
        .map(|b| serde_json::from_slice::<Value>(&b).unwrap())
        .unwrap_or(json!({}));
    let delta = usage
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                json!(
                    v.as_u64()
                        .unwrap()
                        .saturating_sub(previous[k].as_u64().unwrap_or(0))
                ),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    save(&path, &usage);
    save(
        &worker
            .root
            .join(format!("{}-checkpoint-{phase}.json", worker.provider)),
        &json!({
        "result":result,"native_usage_cumulative":usage,"native_usage_since_previous_sample":delta,
        "scope":"interval includes intervening rethink/preparation wakes; result.native_tokens is request delta",
        "model":state.observer.model,"effort":state.observer.effort,"transcript":transcript,
        "native_version":version(&worker.provider),"at_ms":kah_observer::session::now_ms()}),
    );
}
pub fn native_usage(path: &std::path::Path, provider: &str) -> Value {
    super::usage::parse(BufReader::new(fs::File::open(path).unwrap()), provider)
}
pub fn early_finding(first: &Value) -> String {
    first["report"]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["model_claim_not_a_decision"].as_str())
        .find(|s| s.contains("marker") && s.contains("RED"))
        .expect("TRACK2_EARLY_FINDING stage=continuity: no retained marker finding")
        .to_owned()
}
pub fn version(provider: &str) -> String {
    let output = std::process::Command::new(provider)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success(), "TRACK2_VERSION stage=preflight");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
pub fn bind_fast_verdicts(worker: &Worker, commits: &mut [Value]) {
    for id in result_ids(&worker.id) {
        let result = result(&worker.id, &id);
        let request =
            review::workspace::request(&Store::open_wait(&worker.id).unwrap(), &id).unwrap();
        if request.trigger["review_class"] != "commit" {
            continue;
        }
        for commit in commits.iter_mut().filter(|c| c["sha"] == result["head"]) {
            let at = completed_ms(&worker.id, &result);
            if commit["verdict_ms"].as_u64().is_none_or(|prior| at < prior) {
                commit["verdict_ms"] = at.into();
                commit["fast_review_request"] = id.clone().into();
            }
        }
    }
}
