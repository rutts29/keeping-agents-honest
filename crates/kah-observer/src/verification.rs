//! Provider-neutral routing to the host's full-context observer. This flag is
//! written by a primary native host hook, never by a model or guest process.
use crate::{
    judge::{Answer, Judgment},
    session::Store,
};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt};
pub fn enabled(session: &str) -> Result<bool, String> {
    let base = kah_core::run_store::RunStore::default_state_root().map_err(|e| e.to_string())?;
    let path = Store::directory(&base, session).join("review/config.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 || meta.len() > 65536 {
        return Err("REVIEW_CONFIG_PRIVATE stage=verification".into());
    }
    let config: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|_| "REVIEW_CONFIG_JSON stage=verification")?;
    Ok(config["enabled"] == true)
}
/// Routing only. Existing drift questions and reminder thresholds are unchanged.
pub fn trigger(judgment: &Judgment, actions: &[Value]) -> Option<Value> {
    let suspect=matches!(judgment.answers.get("alignment"),Some(Answer::Choice{choice,..}) if choice=="accidental_drift" || choice=="stalled")
        || ["repeating_failed_approach","polishing_instead_of_progress"].iter().any(|key|matches!(judgment.answers.get(*key),Some(Answer::Noul{noul}) if *noul>=crate::judge::questions::STRONG_FAILURE_SIGNAL))
        || matches!(judgment.answers.get("claim_supported"),Some(Answer::Noul{noul}) if *noul<0.5);
    (judgment.backend == "jev" && suspect).then(|| json!({"tripwire":judgment,"actions":actions}))
}

pub fn uncertain_impact(judgment: &Judgment) -> bool {
    use crate::judge::questions as q;
    let steering_uncertain = !matches!(judgment.answers.get("is_steering"),Some(Answer::Noul{noul}) if *noul>=q::STEERING_PROBABILITY);
    let impact = matches!(judgment.answers.get("steer_kind"),Some(Answer::Choice{choice,confidence,probabilities}) if (matches!(choice.as_str(),"goal_change"|"scope_expansion"|"scope_reduction") && (*confidence<q::REPLACE_CONFIDENCE || steering_uncertain)) || (choice=="unclear" && ["goal_change","scope_expansion","scope_reduction"].iter().any(|k|probabilities.get(*k).is_some_and(|p|*p>crate::intake::IGNORE_PROBABILITY))));
    let replacement = match judgment.answers.get("supersedes") {
        Some(Answer::Choice {
            choice,
            confidence,
            probabilities,
        }) => {
            let uncertain = *confidence < q::REPLACE_CONFIDENCE
                || probabilities
                    .get(choice)
                    .is_none_or(|p| *p < q::REPLACE_PROBABILITY)
                || steering_uncertain;
            uncertain
                && (choice != "none"
                    || probabilities
                        .iter()
                        .any(|(id, p)| id != "none" && *p > crate::intake::IGNORE_PROBABILITY))
        }
        _ => false,
    };
    impact || replacement
}
