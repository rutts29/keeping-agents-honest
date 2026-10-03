//! User source order, explicit receipt links, and immutable typing context.
use crate::{intake::Input, verbatim};
use kah_core::intent_history::{Event, History, SourceRef};
use serde_json::{Value, json};

pub fn stored_context(store: &crate::session::Store, input: &Input) -> Result<Value, String> {
    if let Some(prior) = &input.prior_intent {
        return Ok(prior.clone());
    }
    let state = store
        .load()?
        .ok_or("OBSERVER_CHRONOLOGY_STATE stage=chronology")?;
    let history = state
        .run_id
        .as_ref()
        .map(|id| {
            kah_core::run_store::RunStore::open_default(&state.binding.project, id)
                .and_then(|s| s.recover())
                .map(|r| r.history)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    before(history.as_ref(), &store.sources()?, input.key())
}

pub fn before(history: Option<&History>, inputs: &[Input], key: &str) -> Result<Value, String> {
    let position = inputs
        .iter()
        .position(|i| i.key() == key)
        .ok_or("OBSERVER_CHRONOLOGY_SOURCE stage=chronology")?;
    let older = &inputs[..position];
    let past = history.map(|h| historical(h, older)).transpose()?.flatten();
    let Some(history) = past.as_ref() else {
        return Ok(json!({"goal":null,"goal_item_id":null,"items":[]}));
    };
    let goal = Some(history.goal());
    let items = history
        .intents()
        .values()
        .filter(|item| item.is_active())
        .map(|item| json!({"id":item.id,"text":item.text}))
        .collect::<Vec<_>>();
    let goal_id = goal
        .and_then(|goal| {
            items.iter().find(|item| {
                older.iter().any(|input| {
                    input.id() == item["id"] && goal.source_refs.contains(&input.source)
                })
            })
        })
        .map(|item| item["id"].clone());
    Ok(json!({"goal":goal.map(|g|&g.text),"goal_item_id":goal_id,"items":items}))
}
/// Restore lifecycle transitions, not today's item text/status. Evidence does
/// not authorize steering and is omitted from this instruction-only context.
fn historical(history: &History, older: &[Input]) -> Result<Option<History>, String> {
    let mut past = None;
    for record in history.records() {
        if matches!(
            record.event,
            Event::EvidenceAttached { .. } | Event::EvidenceInvalidated { .. }
        ) || !older_authority(
            &serde_json::to_value(&record.event).map_err(|e| e.to_string())?,
            older,
        ) {
            continue;
        }
        if let Event::RunStarted { goal } = &record.event {
            past = Some(History::new(goal.clone()).map_err(|e| e.to_string())?);
        } else if let Some(past) = &mut past {
            past.append(record.event.clone())
                .map_err(|e| format!("OBSERVER_CHRONOLOGY_REPLAY stage=chronology: {e}"))?;
        }
    }
    Ok(past)
}
fn older_authority(value: &Value, older: &[Input]) -> bool {
    if value.get("content_digest").is_some() && value.get("record").is_some() {
        return serde_json::from_value::<SourceRef>(value.clone()).is_ok_and(|s| {
            s.role != kah_core::intent_history::SourceRole::User
                || older.iter().any(|i| i.source == s)
        });
    }
    match value {
        Value::Object(map) => map.values().all(|v| older_authority(v, older)),
        Value::Array(items) => items.iter().all(|v| older_authority(v, older)),
        _ => true,
    }
}
pub fn apply_links(history: &mut History, inputs: &[Input]) -> Result<(), String> {
    for later in inputs {
        let Some(receipt) = later.source.supersedes_receipt.as_deref() else {
            continue;
        };
        let index = inputs
            .iter()
            .position(|i| i.key() == later.key())
            .ok_or("OBSERVER_CHRONOLOGY_SOURCE stage=chronology")?;
        let target=inputs[..index].iter().find(|i|i.source.record.rsplit(':').next()==Some(receipt)).ok_or("OBSERVER_SUPERSEDES_RECEIPT stage=chronology: named receipt is not an older user source")?;
        crate::intake::force(history, &target.text, target.source.clone())?;
        crate::intake::force(history, &later.text, later.source.clone())?;
        if history
            .intent(target.id())
            .is_some_and(|item| item.is_active())
        {
            history
                .append(Event::RequirementWithdrawn {
                    intent_id: verbatim::item_id(&target.source).into(),
                    source_refs: vec![later.source.clone()],
                    approval: None,
                })
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn apply_conflicts(history: &mut History, inputs: &[Input]) -> Result<(), String> {
    use kah_core::intent_history::{Conflict, ConflictAlternative};
    for input in inputs {
        if !history.intent(input.id()).is_some_and(|i| i.is_active()) {
            continue;
        }
        for newer in &input.newer_conflicts {
            if !history
                .intent(newer.as_str())
                .is_some_and(|i| i.is_active())
            {
                continue;
            }
            let other = inputs
                .iter()
                .find(|i| i.id() == *newer)
                .ok_or("OBSERVER_CONFLICT_SOURCE stage=chronology")?;
            let id = format!("chronology-{}-{newer}", input.id());
            if history
                .open_conflicts()
                .keys()
                .any(|key| key.as_str() == id)
            {
                continue;
            }
            let conflict = Conflict::new(
                id,
                vec![input.id().into(), newer.clone().into()],
                vec![
                    ConflictAlternative::new("older", &input.text, vec![input.source.clone()]),
                    ConflictAlternative::new("newer", &other.text, vec![other.source.clone()]),
                ],
                vec![input.source.clone(), other.source.clone()],
            );
            history
                .append(Event::ConflictRaised {
                    conflict,
                    approval: None,
                })
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
