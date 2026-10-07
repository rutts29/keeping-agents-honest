use super::{fixture::save, journey::Worker, telemetry};
use kah_cli::observer_review::{
    self as review,
    state::{Purpose, Request, Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};
#[path = "../relay_support/replay_cases.rs"]
mod cases;

pub fn run(worker: &Worker) -> Value {
    let mut results = vec![];
    let path = worker.root.join("proof-source.jsonl");
    for case in cases::consent_source(&path, &cases::records(&path)) {
        idle(worker);
        let request = Request {
            id: format!(
                "review-{}-{}",
                kah_observer::session::now_ms(),
                std::process::id()
            ),
            worker: worker.id.clone(),
            purpose: Purpose::Drift,
            turn: case.name.clone(),
            head: None,
            intent: case.intent,
            trigger: json!({"historical_replay":case.provenance,"subject":"Original recorded builder actions only; private transport worker is not the subject."}),
            documents: case.documents,
            created_ms: kah_observer::session::now_ms(),
        };
        review::snapshot::wake(&request).unwrap();
        let result = wait(worker, &request.id);
        telemetry::retain_wake(worker, &result, results.len() + 1);
        save(
            &worker
                .root
                .join(format!("{}-{}.json", worker.provider, case.name)),
            &result,
        );
        results.push(json!({"case":case.name,"counterfactual":case.provenance["counterfactual"],"result":result}));
    }
    json!(results)
}
pub fn idle(worker: &Worker) {
    loop {
        review::queue::drain(&worker.id).unwrap();
        let state = Store::read(&worker.id).unwrap();
        consent(worker, state.as_ref());
        if state.as_ref().is_none_or(|s| s.pending.is_none()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
pub fn wait(worker: &Worker, id: &str) -> Value {
    loop {
        if telemetry::result_ids(&worker.id).iter().any(|s| s == id) {
            return telemetry::result(&worker.id, id);
        }
        review::queue::drain(&worker.id).unwrap();
        consent(worker, Store::read(&worker.id).unwrap().as_ref());
        std::thread::sleep(Duration::from_millis(250));
    }
}
fn consent(worker: &Worker, state: Option<&review::state::State>) {
    if let Some(state) = state
        && let Some(id) = &state.observer.pane
    {
        super::journey::consent(
            &kah_cli::run_pane::Pane { id: id.clone() },
            &worker.provider,
            &state.observer.workspace,
        );
    }
}
