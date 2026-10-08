//! Reclassify immutable legacy claim events with the replayed v3 questions.
use super::*;
use crate::observer_review::claims::{ReviewClass, scope};
use kah_observer::judge::{Answer, Judgment};
mod driver;
mod storage;
pub use driver::{cli, run};
pub use storage::recover;

pub fn eligible(wake: &Wake) -> bool {
    supersede::claim_only(wake)
        && wake.trigger["events"].as_array().is_some_and(|events| {
            events
                .iter()
                .any(|e| e["question_set"].as_u64().unwrap_or(0) < scope::VERSION as u64)
        })
}
pub fn reclassify(original: &Wake, judgments: &[Judgment]) -> Result<Option<Wake>> {
    if !eligible(original) {
        return Err("REVIEW_CLAIM_MIGRATION_CLASS stage=queue_migration".into());
    }
    let events = original.trigger["events"].as_array().unwrap();
    if judgments.len() != events.len() {
        return Err("REVIEW_CLAIM_MIGRATION_JUDGMENTS stage=queue_migration".into());
    }
    let mut converted = original.clone();
    let mut class = None;
    let mut updated = vec![];
    for (event, judgment) in events.iter().zip(judgments) {
        validate(judgment)?;
        let next = scope::review_class(judgment);
        if next == Some(ReviewClass::Gate) || class.is_none() {
            class = next;
        }
        let mut event = event.clone();
        event["previous_classification"] = json!({"judgment":event["judgment"],"question_set":event["question_set"],"review_class":event["review_class"]});
        event["judgment"] = serde_json::to_value(judgment)?;
        event["question_set"] = json!(scope::VERSION);
        event["uncertain"] = json!(
            matches!(judgment.answers.get("phase_claim"), Some(Answer::Noul{noul}) if *noul < kah_observer::judge::questions::STEERING_PROBABILITY)
        );
        event["review_class"] = json!(next.map(|c| if c == ReviewClass::Gate {
            "gate"
        } else {
            "commit"
        }));
        updated.push(event);
    }
    let Some(class) = class else { return Ok(None) };
    let head = supersede::trigger_head(&original.trigger).map(str::to_owned);
    converted.trigger["events"] = json!(updated);
    converted.trigger["head"] = json!(head);
    converted.trigger["review_class"] = json!(if class == ReviewClass::Gate {
        "gate"
    } else {
        "commit"
    });
    if class == ReviewClass::Commit {
        converted.trigger["packet_scope"] = json!("diff-v1");
        converted.trigger["review_protocol"] = json!("commit-v1");
    }
    Ok(Some(converted))
}
fn validate(judgment: &Judgment) -> Result<()> {
    if judgment.backend != "jev" || ["phase_claim", "implementation_claim"].iter().any(|id| {
        !matches!(judgment.answers.get(*id), Some(Answer::Noul { noul }) if noul.is_finite() && (0.0..=1.0).contains(noul))
    }) {
        return Err("REVIEW_CLAIM_SCOPE_JUDGMENT stage=queue_migration".into());
    }
    Ok(())
}
