//! Jev screen before the native wake; RuleSource is the replaceable authority seam.
use super::super::state::{Purpose, State, Store};
use super::{authority, capture, input, journal, native, requests};
use crate::Result;
use kah_observer::{
    commit_review,
    context_selection::digest,
    judge::{batch::Batch, jev::HttpClient},
};
use serde_json::{Value, json};
use std::path::Path;
pub fn applicable(purpose: &Purpose, trigger: &Value) -> bool {
    *purpose == Purpose::Checkpoint && trigger.get("captured_snapshot").is_none()
}
pub fn submit(
    store: Store,
    mut state: State,
    home: &Path,
    turn: &str,
    trigger: Value,
) -> Result<Value> {
    let head = head(home, &state, &trigger)?;
    let history = match authority::history(&state.worker.session) {
        Ok(h) => h,
        Err(e) if e.to_string().starts_with("COMMIT_NO_INTENT") => {
            return super::direct::submit(store, state, home, turn, &head, trigger);
        }
        Err(e) => return Err(e),
    };
    let revision =
        kah_core::projection::project(&history, kah_core::projection::ProjectionConfig::default())?
            .revision;
    if let Some(result) = super::carry::admit(&store, &mut state, &head, revision, turn, &trigger)?
    {
        return Ok(result);
    }
    let source = super::current_set::resolve(
        &state.worker.session,
        state.observer.session.as_deref().unwrap_or(""),
        &head,
    )?;
    let history_digest = kah_core::current_rules::digest(history.records())?;
    let source = source.filter(|s| s.history_digest.as_deref() == Some(history_digest.as_str()));
    let Some(source) = source else {
        return super::direct::submit(store, state, home, turn, &head, trigger);
    };
    let checkpoint = trigger["review_class"] != "commit";
    let previous: Option<Value> =
        super::super::state::read(&store.directory.join(if checkpoint {
            "option1-checkpoint-baseline.json"
        } else {
            "option1-baseline.json"
        }))?;
    let base = previous.as_ref().and_then(|p| p["head"].as_str());
    let mut context = capture::project(home, &state, base, &head)?;
    super::binary::publish(&store, &state, &mut context)?;
    let input = input::build(&history, &source, &context, checkpoint)?;
    if super::binary::present(&context) {
        return super::binary::wake(store, state, home, turn, input, trigger);
    }
    crate::unattended_report::save(
        &store.directory,
        &format!("current-rules-commit-{head}.json"),
        &json!({"head":head,"set_digest":source.source_digest,"observer_session":source.observer_session,"intent_revision":input.carried["intent_revision"],"history_digest":source.history_digest}),
    )?;
    if checkpoint {
        return checkpoint_wake(store, state, home, turn, input, trigger);
    }
    screen(store, state, home, turn, input, trigger)
}
fn head(home: &Path, state: &State, trigger: &Value) -> Result<String> {
    if let Some(head) = trigger["head"].as_str() {
        return Ok(head.into());
    }
    super::super::tags::head(home, state)?
        .ok_or_else(|| "COMMIT_HEAD_MISSING stage=admission".into())
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
    journal::save_screen(&record)?;
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
