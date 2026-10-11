//! Code-resolved, verbatim authority. No thinker, atomization or observer ID.
use crate::Result;
use kah_core::{current_rules::Source, intent_history::History};
use kah_observer::commit_check::{self, Authority, Input, Rule};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn build(history: &History, standing: &[Source], context: &Value) -> Result<Input> {
    let head = context["head"]
        .as_str()
        .ok_or("COMMIT_HEAD_MISSING stage=input")?;
    let mut intent = commit_check::CurrentIntent::resolve(history, BTreeMap::new())?;
    if !history.snapshot().open_conflicts.is_empty() {
        return Err("UNTRUSTED_AUTHORITY_CONFLICT stage=commit_source".into());
    }
    if intent.goal.trim().is_empty() {
        return Err("UNTRUSTED_AUTHORITY_MISSING stage=commit_source".into());
    }
    for source in standing {
        if source.origin["kind"] != "standing_file"
            || source.content_digest != commit_check::digest(source.text.as_bytes())
            || intent.items.contains_key(&source.id)
        {
            return Err("UNTRUSTED_AUTHORITY_SOURCE stage=commit_source".into());
        }
        intent.items.insert(source.id.clone(), Authority {
            text: source.text.clone(), origin: "standing_file".into(),
            sources: json!({"source_id":source.id,"content_digest":source.content_digest,"origin":source.origin}),
        });
    }
    let (hunks, files) = super::super::option1::input::files(context, head)?;
    let rules = intent
        .items
        .keys()
        .map(|id| {
            (
                id.clone(),
                Rule {
                    authority: id.clone(),
                    unconditional_prohibition: true,
                    prohibition: false,
                    literal_check: true,
                    hunks: hunks.keys().cloned().collect(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let authority_digest = kah_observer::context_selection::digest(&json!(intent));
    Ok(Input {
        revision: head.into(),
        baseline: context["baseline_tree"].as_str().map(str::to_owned),
        checkpoint: false,
        code_changed: !hunks.is_empty(),
        impacted: rules.keys().cloned().collect(),
        intent,
        rules,
        hunks,
        files,
        matrix: vec![],
        carried: json!({"authority_digest":authority_digest,"source":"code_resolved_verbatim","domain":"untrusted","observer_session":null}),
    })
}
