use super::state::Request;
use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::{
    engine,
    intake::{Decision, Input},
    judge::{self, Judgment},
    session::Store,
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub fn pending(worker: &str) -> Result<Option<Value>> {
    let store = Store::open(&RunStore::default_state_root()?, worker, Duration::ZERO)?;
    Ok(store
        .sources()?
        .into_iter()
        .find(|i| {
            !i.dismissed && !i.promoted && matches!(i.decision, Some(Decision::Verification { .. }))
        })
        .map(|i| json!({"uncertain_user_input":i})))
}
pub fn context(worker: &str, source: &Input) -> Result<Value> {
    let store = Store::open(
        &RunStore::default_state_root()?,
        worker,
        Duration::from_secs(2),
    )?;
    context_from(&store, source)
}
fn context_from(store: &Store, source: &Input) -> Result<Value> {
    let prior = kah_observer::chronology::stored_context(store, source)?;
    let sources = store.sources()?;
    let position = sources
        .iter()
        .position(|i| i.key() == source.key())
        .ok_or("REVIEW_STEERING_SOURCE stage=context")?;
    let newer = sources[position + 1..]
        .iter()
        .filter(|later| {
            later.source.supersedes_receipt.as_deref() != source.source.record.rsplit(':').next()
                && !later.dismissed
                && (later.promoted
                    || later.decision.as_ref().is_none_or(|d| {
                        d.records_intent()
                            || matches!(d, Decision::Verification { .. } | Decision::Gap { .. })
                    }))
        })
        .map(|i| json!({"id":i.id(),"text":i.text,"source":i.source}))
        .collect::<Vec<_>>();
    Ok(json!({"prior_intent":prior,"newer":newer}))
}
pub fn verify_proposal(
    request: &Request,
    report: &kah_observer::review_evidence::Report,
) -> Result<Option<Value>> {
    super::failure::validation(super::failure::Origin::ReportValidation, || {
        validate_proposal(request, report)
    })
}
fn validate_proposal(
    request: &Request,
    report: &kah_observer::review_evidence::Report,
) -> Result<Option<Value>> {
    let Some(proposal) = report
        .decision_units
        .as_ref()
        .and_then(|u| u.steering.as_ref())
    else {
        return Ok(None);
    };
    if request.purpose != super::state::Purpose::Steering {
        return Err("REVIEW_STEERING_PURPOSE stage=report".into());
    }
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let context = &request.trigger["steering_context"];
    if !context["prior_intent"]["items"].is_array() || !context["newer"].is_array() {
        return Err(
            "REVIEW_STEERING_CONTEXT stage=report: host chronological context missing".into(),
        );
    }
    if proposal
        .supersedes
        .as_deref()
        .is_some_and(|id| context["prior_intent"]["goal_item_id"] == id)
        && proposal.kind != kah_observer::steering_proposal::Kind::GoalChange
    {
        return Err("REVIEW_STEERING_GOAL_TARGET stage=report: partial corrections cannot replace the whole original goal".into());
    }
    let prior = context["prior_intent"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let newer = context["newer"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    Ok(Some(proposal.verify(
        &request.documents,
        &source.source.record,
        &prior,
        &newer,
    )?))
}
pub fn covered(worker: &str, trigger: &Value) -> Result<bool> {
    let source: Input = serde_json::from_value(trigger["uncertain_user_input"].clone())?;
    let store = match Store::open(&RunStore::default_state_root()?, worker, Duration::ZERO) {
        Ok(store) => store,
        Err(error) if error.starts_with("OBSERVER_BUSY") => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(store.sources()?.iter().any(|current| {
        current.key() == source.key()
            && current.text == source.text
            && current.source == source.source
            && (current.dismissed
                || current.promoted
                || matches!(
                    current.decision,
                    Some(Decision::Jev { .. } | Decision::Validated { .. })
                ))
    }))
}
pub fn judge(home: &Path, request: &Request, actions: &[Value]) -> Result<(Judgment, Value)> {
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let store = Store::open(
        &RunStore::default_state_root()?,
        &request.worker,
        Duration::from_secs(2),
    )?;
    let prior = kah_observer::chronology::stored_context(&store, &source)?;
    drop(store);
    let items = prior["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some((i["id"].as_str()?.to_owned(), i["text"].as_str()?.to_owned()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut input = judge::questions::steering_state(
        prior["goal"].as_str(),
        prior["goal_item_id"].as_str(),
        &items,
        &source.text,
        "",
    );
    input["full_context_verified_evidence"] = json!(actions);
    crate::privacy::scrub(&mut input);
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (judgment, input) =
        super::preparation::evaluate(home, request, &input, &judge::questions::steering(&items))?;
    super::steering_conflict::check(home, request)?;
    Ok((judgment, input))
}
/// Preserve any intervening explicit user review. The next native event applies
/// the verified decision through the existing verbatim ledger machinery.
pub fn apply(request: &Request, judgment: &Judgment) -> Result<&'static str> {
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let mut store = Store::open(
        &RunStore::default_state_root()?,
        &request.worker,
        Duration::from_secs(2),
    )?;
    let Some(current) = store
        .sources()?
        .into_iter()
        .find(|i| i.key() == source.key())
    else {
        return Err("REVIEW_STEERING_SOURCE stage=steering".into());
    };
    if current.dismissed
        || current.promoted
        || !matches!(current.decision, Some(Decision::Verification { .. }))
    {
        return Ok("explicit_user_review_preserved");
    }
    if current.text != source.text || current.source != source.source {
        return Err("REVIEW_STEERING_SOURCE_CHANGED stage=steering".into());
    }
    let state = store.load()?.ok_or("REVIEW_WORKER_STATE stage=steering")?;
    let current = serde_json::to_value(engine::projection(&state)?)?;
    let previous: Value = serde_json::from_str(
        &request
            .documents
            .iter()
            .find(|d| d.id == "ledger")
            .ok_or("REVIEW_LEDGER_MISSING stage=steering")?
            .text,
    )?;
    if current["revision"] != previous["revision"] {
        return Ok("intent_changed_recheck_next_turn");
    }
    store.source_decision(
        source.key(),
        &Decision::Jev {
            judgment: Box::new(judgment.clone()),
        },
    )?;
    store.append(&json!({"kind":"judgment","purpose":"verified_steering","source":"observer","question_set":judge::questions::VERSION,"judgment":judgment,"request":request.id}))?;
    Ok("verified_for_next_native_event")
}

/// Apply only the actual validated operation; never manufacture legacy scores.
pub fn apply_proposal(
    request: &Request,
    proposal: &kah_observer::steering_proposal::Proposal,
    judgment: &Judgment,
) -> Result<&'static str> {
    if !kah_observer::steering_proposal::validated(proposal, judgment) {
        return Err("REVIEW_STEERING_UNVALIDATED stage=steering".into());
    }
    let source: Input = serde_json::from_value(request.trigger["uncertain_user_input"].clone())?;
    let mut store = Store::open(
        &RunStore::default_state_root()?,
        &request.worker,
        Duration::from_secs(2),
    )?;
    let current = store
        .sources()?
        .into_iter()
        .find(|i| i.key() == source.key())
        .ok_or("REVIEW_STEERING_SOURCE stage=steering")?;
    if current.dismissed
        || current.promoted
        || !matches!(current.decision, Some(Decision::Verification { .. }))
    {
        return Ok("explicit_user_review_preserved");
    }
    if current.text != source.text || current.source != source.source {
        return Err("REVIEW_STEERING_SOURCE_CHANGED stage=steering".into());
    }
    let state = store.load()?.ok_or("REVIEW_WORKER_STATE stage=steering")?;
    let projection = serde_json::to_value(engine::projection(&state)?)?;
    let previous: Value = serde_json::from_str(
        &request
            .documents
            .iter()
            .find(|d| d.id == "ledger")
            .ok_or("REVIEW_LEDGER_MISSING stage=steering")?
            .text,
    )?;
    if projection["revision"] != previous["revision"]
        || context_from(&store, &source)? != request.trigger["steering_context"]
    {
        return Ok("intent_changed_recheck_next_turn");
    }
    let conflicts = proposal
        .newer_conflicts
        .iter()
        .filter(|c| c.conflicts)
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    store.source_conflicts(source.key(), &conflicts, judgment)?;
    store.source_decision(
        source.key(),
        &Decision::Validated {
            proposal: Box::new(proposal.clone()),
            judgment: Box::new(judgment.clone()),
        },
    )?;
    store.append(&json!({"kind":"judgment","purpose":"verified_steering","source":"observer","question_set":kah_observer::decision_units::VERSION,"judgment":judgment,"request":request.id,"proposal":proposal}))?;
    Ok("verified_for_next_native_event")
}
