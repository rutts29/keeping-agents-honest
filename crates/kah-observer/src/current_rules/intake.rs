//! Immutable verification inputs join word accounting; resolution is observer evidence.
use super::*;
use crate::{intake::Decision, session::Store};
use kah_core::{
    current_rules::{Disposition, Input},
    intent_history::{SourceOrigin, is_kah_delegate_message},
};

pub fn sources(binding: &Binding) -> Result<Vec<Source>, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let store = Store::open_wait(&base, &binding.session)?;
    Ok(store
        .sources()?
        .iter()
        .filter(|i| {
            matches!(i.decision, Some(Decision::Verification { .. }))
                && ((!i.dismissed && !i.promoted) || i.rule_set_resolution.is_some())
        })
        .map(|i| {
            Source::intake(
                i.key(),
                &i.text,
                &i.source,
                i.source.origin != Some(SourceOrigin::Relayed) && is_kah_delegate_message(&i.text),
            )
        })
        .collect())
}

pub fn resolve(
    binding: &Binding,
    set: &ValidatedSet,
    input: &Input,
) -> Result<serde_json::Value, String> {
    if set.as_set().input_digest != input.digest()? {
        return Err("RULE_SET_BINDING stage=intake_resolution: input changed".into());
    }
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let mut store = Store::open_wait(&base, &binding.session)?;
    let mut promoted = 0;
    let mut dismissed = 0;
    for record in store.sources()? {
        if record.dismissed
            || record.promoted
            || !matches!(record.decision, Some(Decision::Verification { .. }))
        {
            continue;
        }
        let Some(source) = input
            .sources
            .iter()
            .find(|s| s.id == format!("intake:{}", record.key()))
        else {
            continue;
        };
        if source.text != record.text
            || source.origin["source"]
                != serde_json::to_value(&record.source).map_err(|e| e.to_string())?
        {
            continue;
        }
        let Some((action, clauses)) = resolution(set, source) else {
            continue;
        };
        if action == "dismiss" {
            dismissed += 1;
        } else {
            promoted += 1;
        }
        let reason = clauses
            .iter()
            .map(|c| c.reason.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        store.append(&json!({"kind":"source_review_v1","record":record.key(),"action":action,
            "source":"observer_rule_set","rule_set_revision":set.revision(),"rule_set_digest":set.digest()?,"reason":reason}))?;
    }
    Ok(
        json!({"promoted":promoted,"dismissed":dismissed,"authority":"observer evidence; history and stored classification unchanged"}),
    )
}

fn resolution<'a>(
    set: &'a ValidatedSet,
    source: &Source,
) -> Option<(&'static str, Vec<&'a kah_core::current_rules::Clause>)> {
    let clauses = set
        .as_set()
        .clauses
        .iter()
        .filter(|c| c.words.source == source.id)
        .collect::<Vec<_>>();
    if clauses.is_empty()
        || clauses
            .iter()
            .any(|c| c.disposition == Disposition::UnresolvedConflict)
    {
        return None;
    }
    if source.origin["non_authority_sender"] == true
        && clauses
            .iter()
            .all(|c| c.disposition == Disposition::RejectedJunk)
    {
        Some(("dismiss", clauses))
    } else if source.origin["non_authority_sender"] == false
        && clauses
            .iter()
            .all(|c| c.disposition != Disposition::RejectedJunk)
    {
        Some(("promote", clauses))
    } else {
        None
    }
}
