//! Jev screen before the native wake; RuleSource is the replaceable authority seam.
use super::super::state::{Purpose, State, Store};
use super::{authority, capture, input, journal, native, requests};
use crate::Result;
use kah_observer::{
    commit_review,
    context_selection::digest,
    judge::{batch::Batch, jev::HttpClient},
    rule_source::{RuleList, Source},
};
use serde_json::{Value, json};
use std::path::Path;
pub fn applicable(purpose: &Purpose, trigger: &Value) -> bool {
    *purpose == Purpose::Checkpoint && trigger.get("captured_snapshot").is_none()
}
pub fn submit(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    trigger: Value,
) -> Result<Value> {
    let history = authority::history(&state.worker.session)?;
    let items = authority::items(&history);
    let head =
        super::super::tags::head(home, &state)?.ok_or("COMMIT_HEAD_MISSING stage=admission")?;
    let head = trigger["head"].as_str().unwrap_or(&head).to_owned();
    let list = super::super::state::read::<RuleList>(&store.directory.join("option1-rules.json"))?;
    let source = list
        .as_ref()
        .filter(|l| state.observer.session.as_ref() == Some(&l.observer_session))
        .and_then(|l| l.load(&items).ok());
    let Some(source) = source else {
        return native::submit(
            store,
            state,
            home,
            turn,
            &head,
            json!({"stage":"rules","items":items,"original_trigger":trigger}),
            trigger.clone(),
        );
    };
    let checkpoint = trigger["review_class"] != "commit";
    let previous: Option<Value> =
        super::super::state::read(&store.directory.join(if checkpoint {
            "option1-checkpoint-baseline.json"
        } else {
            "option1-baseline.json"
        }))?;
    let base = previous.as_ref().and_then(|p| p["head"].as_str());
    let context = capture::project(home, &state, base, &head)?;
    let input = input::build(&history, &source, &context, checkpoint)?;
    if checkpoint {
        return checkpoint_wake(store, state, home, turn, input, trigger);
    }
    screen(store, state, home, turn, input, trigger)
}

fn checkpoint_wake(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    input: kah_observer::commit_check::Input,
    trigger: Value,
) -> Result<Value> {
    let covered: Option<Value> =
        super::super::state::read(&store.directory.join("option1-checkpoint-baseline.json"))?;
    let records = journal::all(&state.worker.session)?
        .into_iter()
        .filter(|r| {
            !covered.as_ref().is_some_and(|c| {
                c["covered_commits"]
                    .as_array()
                    .is_some_and(|heads| heads.iter().any(|h| h == &r.head))
            })
        })
        .collect::<Vec<_>>();
    let files = input
        .files
        .iter()
        .map(|(p, f)| (p.clone(), f.digest.clone()))
        .collect();
    let authority = commit_review::authority(&input);
    let plan = commit_review::checkpoint::plan(&records, &authority, &files);
    let head = input.revision.clone();
    native::submit(
        store,
        state,
        home,
        turn,
        &head,
        json!({"stage":"checkpoint","input":input,"plan":plan,"original_trigger":trigger}),
        trigger.clone(),
    )
}

fn screen(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    input: kah_observer::commit_check::Input,
    trigger: Value,
) -> Result<Value> {
    let rate = kah_core::run_store::RunStore::default_state_root()?.join("jev-rate");
    let calibration = Batch::calibration(&rate)?;
    let (requests, map) = requests::build(&input, calibration.as_ref())?;
    crate::unattended_report::save(
        &store.directory,
        &format!("option1-keys-{}.json", input.revision),
        &map,
    )?;
    if super::selection::needed(&requests, calibration.as_ref())? {
        return super::selection::wake(store, state, home, turn, input, (requests, None), trigger);
    }
    let batch = Batch::new(
        &store.directory.join("literal").join(&input.revision),
        &rate,
    )?;
    let record = commit_review::transport::evaluate(
        &state.worker.session,
        &input.revision,
        &journal::policy(home)?,
        &batch,
        &requests,
        &HttpClient::from_home(home)?,
    )?;
    journal::save(&record)?;
    if record.requires_native()
        && let Some(fold) = super::fold::into_due(home, &state, &record)?
    {
        return Ok(json!({"status":"option1_reviewed","head":input.revision,"folded":fold}));
    }
    if !record.gaps.is_empty() && !requests.is_empty() {
        let rejected = requests
            .into_iter()
            .filter(|r| record.gaps.iter().any(|g| g["request"] == r.id))
            .collect();
        return super::selection::wake(
            store,
            state,
            home,
            turn,
            input,
            (rejected, Some(record)),
            trigger,
        );
    }
    if record.requires_native() {
        let job = json!({"stage":"confirmation","input":input,"record":record,"original_trigger":trigger});
        return native::submit(store, state, home, turn, &input.revision, job, trigger);
    }
    crate::unattended_report::save(
        &store.directory,
        "option1-baseline.json",
        &json!({"head":input.revision,"reviewed":"jev_only","authority":digest(&serde_json::to_value(&input.intent)?)}),
    )?;
    Ok(json!({"status":"option1_reviewed","head":input.revision,"woke":false,"record":record}))
}
