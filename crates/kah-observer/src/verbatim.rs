//! Jev chooses typed operations; code copies exact source text into history.
use crate::judge::{Answer, Judgment};
use kah_core::intent_history::*;
use sha2::{Digest, Sha256};

pub const STEERING_PROBABILITY: f64 = 0.8;
pub const TYPE_CONFIDENCE: f64 = 0.7;
pub const REPLACE_CONFIDENCE: f64 = 0.85;
pub const REPLACE_PROBABILITY: f64 = 0.9;

pub fn apply(
    history: &mut History,
    text: &str,
    source: SourceRef,
    judgment: &Judgment,
) -> Result<(), String> {
    let mut next = history.clone();
    apply_inner(&mut next, text, source, judgment)?;
    *history = next;
    Ok(())
}
fn apply_inner(
    history: &mut History,
    text: &str,
    source: SourceRef,
    judgment: &Judgment,
) -> Result<(), String> {
    if source.role != SourceRole::User || judgment.backend != "jev" {
        return Err("OBSERVER_AUTHORITY stage=verbatim".into());
    }
    if let Some(item) = history.intent(item_id(&source)) {
        return if item.text == text && item.introduced_by == [source] {
            Ok(())
        } else {
            Err("OBSERVER_SOURCE_REWRITE stage=verbatim".into())
        };
    }
    let Some(Answer::Noul { noul }) = judgment.answers.get("is_steering") else {
        return Err("OBSERVER_TYPING stage=verbatim".into());
    };
    if *noul <= 1.0 - STEERING_PROBABILITY {
        return Ok(());
    }
    let (kind, probability, confidence) = choice(judgment, "steer_kind")?;
    if *noul < STEERING_PROBABILITY {
        return Err("OBSERVER_TYPING_UNCERTAIN stage=verbatim".into());
    }
    let kind_certain = probability >= TYPE_CONFIDENCE
        && confidence >= TYPE_CONFIDENCE
        && !matches!(kind, "unclear" | "none" | "question_only");
    let (target, probability, confidence) = if judgment.answers.contains_key("supersedes") {
        choice(judgment, "supersedes")?
    } else {
        ("none", 1.0, 1.0)
    };
    let replacing = matches!(kind, "correction" | "scope_reduction" | "goal_change");
    let selected = history.intent(target).filter(|item| {
        item.is_active()
            || item.lifecycle == IntentLifecycle::Withdrawn && item.changed_by.contains(&source)
    });
    if target != "none" && selected.is_none() {
        return Err("OBSERVER_TARGET stage=verbatim".into());
    }
    apply_operation(
        history,
        text,
        source,
        kind,
        target,
        probability >= REPLACE_PROBABILITY
            && confidence >= REPLACE_CONFIDENCE
            && replacing
            && kind_certain,
        !kind_certain || probability < REPLACE_PROBABILITY || confidence < REPLACE_CONFIDENCE,
    )
}

fn apply_operation(
    history: &mut History,
    text: &str,
    source: SourceRef,
    kind: &str,
    target: &str,
    replace: bool,
    uncertain: bool,
) -> Result<(), String> {
    let selected = history.intent(target);
    let mut item = IntentItem::new(
        item_id(&source),
        intent_kind(kind)?,
        text,
        vec![source.clone()],
    );
    let event = if let Some(selected) = selected.filter(|_| replace) {
        let previous_id = selected.id.clone();
        if kind == "scope_reduction" {
            if selected.is_active() {
                history
                    .append(Event::RequirementWithdrawn {
                        intent_id: previous_id,
                        source_refs: vec![source],
                        approval: None,
                    })
                    .map_err(|e| e.to_string())?;
            }
            Event::RequirementAdded {
                item,
                approval: None,
            }
        } else {
            item.supersedes.push(previous_id.clone());
            Event::RequirementSuperseded {
                previous_id,
                replacement: item,
                approval: None,
            }
        }
    } else {
        if uncertain
            || matches!(kind, "correction" | "scope_reduction" | "goal_change")
            || target != "none"
        {
            item.uncertain_supersedes = history
                .intents()
                .values()
                .filter(|i| i.is_active())
                .map(|i| i.id.clone())
                .collect();
        }
        Event::RequirementAdded {
            item,
            approval: None,
        }
    };
    history.append(event).map_err(|e| e.to_string())?;
    Ok(())
}

fn intent_kind(kind: &str) -> Result<IntentKind, String> {
    match kind {
        "new_constraint" | "correction" | "none" | "question_only" | "unclear" => {
            Ok(IntentKind::Constraint)
        }
        "scope_expansion" | "scope_reduction" => Ok(IntentKind::Scope),
        "initial_task" | "goal_change" => Ok(IntentKind::Outcome),
        _ => Err("OBSERVER_TYPE_UNKNOWN stage=verbatim".into()),
    }
}
fn choice<'a>(judgment: &'a Judgment, key: &str) -> Result<(&'a str, f64, f64), String> {
    match judgment.answers.get(key) {
        Some(Answer::Choice {
            choice,
            probabilities,
            confidence,
        }) => Ok((
            choice,
            *probabilities.get(choice).unwrap_or(&0.0),
            *confidence,
        )),
        _ => Err("OBSERVER_TYPING stage=verbatim".into()),
    }
}
pub fn item_id(source: &SourceRef) -> String {
    format!(
        "user-{:x}",
        Sha256::digest(format!("{}\0{}", source.record, source.content_digest).as_bytes())
    )
}
