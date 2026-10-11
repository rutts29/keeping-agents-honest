//! Whole-record preparation at one persistent native wake, no text slicing.
use super::super::state::{State, Store};
use super::{journal, native};
use crate::Result;
use kah_observer::{
    commit_review,
    context_selection::digest,
    judge::batch::{Batch, Tokens, WireRequest},
    whole_preparation::{self, Manifest, Outcome, Policy},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
pub fn needed(
    requests: &[WireRequest],
    calibration: Option<&kah_observer::context_budget::Calibration>,
) -> Result<bool> {
    for request in requests {
        let value: Value = serde_json::from_str(&request.body)?;
        let questions = serde_json::from_value(value["questions"].clone())?;
        if calibration
            .and_then(|c| c.plan(&value["state"], &questions))
            .is_some_and(|p| p.prepare)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn wake(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    input: kah_observer::commit_check::Input,
    evidence: (Vec<WireRequest>, Option<commit_review::Review>),
    trigger: Value,
) -> Result<Value> {
    let (requests, record) = evidence;
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    let calibration = Batch::calibration(&rate)?;
    let catalogues = requests
        .iter()
        .map(|r| super::whole::catalogue(&serde_json::from_str(&r.body)?))
        .collect::<Result<Vec<_>>>()?;
    let head = input.revision.clone();
    let job = json!({"stage":"selection","input":input,"requests":requests,"catalogues":catalogues,"calibration":calibration,"record":record,"original_trigger":trigger});
    native::submit(store, state, home, turn, &head, job, trigger)
}
pub fn continue_review(
    store: Store,
    mut state: State,
    home: &Path,
    turn: &str,
    job: &Value,
    manifests: Vec<Manifest>,
) -> Result<Value> {
    let requests: Vec<WireRequest> = serde_json::from_value(job["requests"].clone())?;
    if requests.len() != manifests.len() {
        return Err("COMMIT_SELECTION_ACCOUNTING stage=report".into());
    }
    let input: kah_observer::commit_check::Input = serde_json::from_value(job["input"].clone())?;
    let previous: Option<commit_review::Review> = serde_json::from_value(job["record"].clone())?;
    let known: BTreeSet<_> = previous
        .as_ref()
        .into_iter()
        .flat_map(|r| &r.gaps)
        .filter_map(|g| g["body_digest"].as_str().map(str::to_owned))
        .collect();
    let policy = Policy {
        calibration: serde_json::from_value(job["calibration"].clone())?,
        known_oversized: known,
    };
    let (rendered, gaps) = render(job, &requests, &manifests, &policy, &input.revision)?;
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    let batch = Batch::new(
        &store.directory.join("literal").join(&input.revision),
        &rate,
    )?;
    let mut record = commit_review::transport::evaluate(
        &state.worker.session,
        &input.revision,
        &journal::policy(home)?,
        &batch,
        &rendered,
        &kah_observer::judge::jev::HttpClient::from_home(home)?,
    )?;
    if let Some(old) = previous {
        record.answers.extend(old.answers);
    }
    let answers = record
        .answers
        .iter()
        .map(|a| serde_json::to_value(a).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    let retained_gaps = record.gaps;
    record = commit_review::classify(
        &state.worker.session,
        &input.revision,
        &answers,
        &record.policy,
    )?;
    record.gaps = retained_gaps;
    record.gaps.extend(gaps);
    journal::save(&record)?;
    let trigger = job["original_trigger"].clone();
    state.pending = None; // Persist only the successor; a failed Jev call retains the old request.
    native::submit(
        store,
        state,
        home,
        turn,
        &input.revision.clone(),
        json!({"stage":"confirmation","input":input,"record":record,"selection":manifests,"original_trigger":trigger}),
        trigger,
    )
}

fn render(
    job: &Value,
    requests: &[WireRequest],
    manifests: &[Manifest],
    policy: &Policy,
    head: &str,
) -> Result<(Vec<WireRequest>, Vec<Value>)> {
    let mut rendered = vec![];
    let mut gaps = vec![];
    for (index, (request, manifest)) in requests.iter().zip(manifests).enumerate() {
        let body: Value = serde_json::from_str(&request.body)?;
        let catalogue = serde_json::from_value(job["catalogues"][index].clone())?;
        match whole_preparation::prepare(&body, &catalogue, manifest, policy)? {
            Outcome::Rendered { body, .. } => {
                let text = serde_json::to_string(&body)?;
                rendered.push(WireRequest {
                    id: format!("commit-{}-{}", head, digest(&body)),
                    tokens: Tokens::estimate(&text, policy.calibration.as_ref())?,
                    body: text,
                });
            }
            Outcome::DirectObserverReview { gap } => gaps.push(serde_json::to_value(gap)?),
        }
    }

    Ok((rendered, gaps))
}
