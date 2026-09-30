//! Explicit local user decisions over source history. No model call is involved.
use crate::{
    engine::reconcile,
    inspect_state::InspectSource,
    intake::Input,
    session::{State, Store},
};
use kah_core::{
    intent_history::{
        Approval, Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef,
        SourceRole,
    },
    projection::{ActiveProjection, ProjectionConfig},
    run_store::RunStore,
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub fn list(session: &str) -> Result<Value, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let store = Store::open(&base, session, Duration::from_secs(2))?;
    let state = store.load()?.ok_or("REVIEW_SESSION stage=review")?;
    let history = history(&state)?;
    let sources = store.sources()?;
    let mut ignored = 0;
    let items=sources.iter().rev().filter_map(|input|{
        let status=input.status(history.as_ref());
        if !matches!(status,"review_required"|"replacement_review") {
            if !matches!(status,"ignored"|"aside"|"gap") || ignored>=12{return None;} ignored+=1;
        }
        Some(json!({"id":input.id(),"text":input.text,"source":input.source,"status":status,"actions":if matches!(status,"review_required"|"replacement_review"){vec!["confirm","dismiss"]}else{vec!["promote","dismiss"]}}))
    }).collect::<Vec<_>>();
    Ok(json!({"session":session,"items":items}))
}
pub fn apply(session: &str, action: &str, id: &str) -> Result<(), String> {
    if !matches!(action, "confirm" | "dismiss" | "promote") {
        return Err("REVIEW_ACTION stage=review".into());
    }
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let mut store = Store::open(&base, session, Duration::from_secs(30))?;
    let mut state = store.load()?.ok_or("REVIEW_SESSION stage=review")?;
    let inputs = store.sources()?;
    let input = inputs
        .iter()
        .find(|i| i.id() == id)
        .ok_or("REVIEW_SOURCE_UNKNOWN stage=review")?;
    let source = inspect_source(&state, &inputs)?;
    let previous = source
        .open(&state.binding.project)
        .map_err(|e| e.to_string())?
        .map(|r| r.recovered);
    let mut history = previous.as_ref().map(|r| r.history.clone());
    let receipt = receipt(session, action, id);
    apply_history(&mut history, input, action, &receipt)?;
    if let Some(history) = &history {
        let mut projection = ActiveProjection::from_history(history, ProjectionConfig::default())
            .map_err(|e| e.to_string())?;
        projection.generated_by.component = "kah.user-controlled-verbatim".into();
        reconcile::persist(
            &state.binding.project,
            &source,
            previous.as_ref(),
            history,
            &projection,
        )?;
        state.run_id = Some(source.run_id().to_string());
    }
    store.append(&json!({"kind":"source_review_v1","record":input.source.record,"action":action,"source":"user","receipt":receipt}))?;
    state.compaction = None;
    state.action_digest = None;
    state.drift_active = false;
    state.gap = None;
    store.save(&state)
}
fn apply_history(
    history: &mut Option<History>,
    input: &Input,
    action: &str,
    receipt: &SourceRef,
) -> Result<(), String> {
    if action == "promote" {
        let h = history.get_or_insert(
            History::new(GoalRevision::new(
                1,
                &input.text,
                vec![input.source.clone()],
            ))
            .map_err(|e| e.to_string())?,
        );
        if h.intent(input.id()).is_some() {
            return Err("REVIEW_ALREADY_INTENT stage=review".into());
        }
        let mut item = IntentItem::new(
            input.id(),
            IntentKind::Constraint,
            &input.text,
            vec![input.source.clone()],
        );
        item.changed_by.push(receipt.clone());
        item.user_confirmed = true;
        h.append(Event::RequirementAdded {
            item,
            approval: Some(Approval::new(receipt.clone())),
        })
        .map_err(|e| e.to_string())?;
    } else if let Some(h) = history {
        match h.intent(input.id()) {
            Some(item) if item.is_active() => {
                let event = if action == "confirm" {
                    Event::RequirementConfirmed {
                        intent_id: input.id().into(),
                        source_refs: vec![receipt.clone()],
                    }
                } else {
                    Event::RequirementWithdrawn {
                        intent_id: input.id().into(),
                        source_refs: vec![receipt.clone()],
                        approval: None,
                    }
                };
                h.append(event).map_err(|e| e.to_string())?;
            }
            _ if action == "confirm" => return Err("REVIEW_NOT_ACTIVE stage=review".into()),
            _ => (),
        }
    } else if action == "confirm" {
        return Err("REVIEW_NOT_ACTIVE stage=review".into());
    }
    Ok(())
}
fn inspect_source(state: &State, inputs: &[Input]) -> Result<InspectSource, String> {
    let first = inputs.first().ok_or("REVIEW_SOURCE_EMPTY stage=review")?;
    let project = state
        .binding
        .project
        .canonicalize()
        .map_err(|e| format!("REVIEW_PROJECT stage=review: {e}"))?;
    let digest = format!(
        "sha256:{:x}",
        Sha256::digest(project.as_os_str().as_encoded_bytes())
    );
    let records = inputs
        .iter()
        .map(|i| BundleRecord::new(&i.source.record, SourceRole::User, &i.text))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let bundle = NeutralSessionBundle::from_records(
        first.source.provider.clone(),
        &state.binding.session,
        digest,
        records,
    )
    .map_err(|e| e.to_string())?;
    InspectSource::from_bundle(&bundle).map_err(|e| e.to_string())
}
fn history(state: &State) -> Result<Option<History>, String> {
    state
        .run_id
        .as_ref()
        .map(|id| {
            RunStore::open_default(&state.binding.project, id)
                .and_then(|s| s.recover())
                .map(|r| r.history)
                .map_err(|e| e.to_string())
        })
        .transpose()
}
fn receipt(session: &str, action: &str, id: &str) -> SourceRef {
    let payload = json!({"action":action,"item":id,"at_ms":crate::session::now_ms()}).to_string();
    let digest = format!("sha256:{:x}", Sha256::digest(payload.as_bytes()));
    SourceRef::new(
        SourceProvider::Bundle,
        session,
        format!("kah-user:review:{action}:{}", &digest[7..31]),
        SourceRole::User,
        digest,
    )
}
