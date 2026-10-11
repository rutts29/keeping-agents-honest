//! Jev-only commit checks. Raw flags are advisory and never native confirmation.
use crate::Result;
use kah_observer::{
    commit_review::{self, DoubtLine, Review},
    context_budget::Calibration,
    judge::batch::{Batch, Transport, WireRequest},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub mod host;
pub mod input;
mod retry;
pub mod worker;

#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub domain: String,
    pub record: Review,
    pub key_map: Value,
    pub flagged: bool,
    pub native_wake_count: u64,
    pub status: String,
}
pub fn evaluate(
    worker: &str,
    head: &str,
    policy: &DoubtLine,
    batch: &Batch,
    requests: &[WireRequest],
    key_map: Value,
    transport: &impl Transport,
) -> Result<Report> {
    let mut record =
        commit_review::transport::evaluate(worker, head, policy, batch, requests, transport)?;
    let flagged = record.woke;
    record.woke = false; // Threshold classification is not an actual native wake.
    for gap in &mut record.gaps {
        gap["route"] = json!("recorded_untrusted_gap");
        gap["status"] = json!(if gap["known_oversized"] == true {
            "commit not checked: too large"
        } else {
            "commit not checked: missing authority"
        });
    }
    Ok(report(record, key_map, flagged))
}
pub fn gap(worker: &str, head: &str, policy: &DoubtLine, reason: &str) -> Result<Report> {
    let mut record = commit_review::classify(worker, head, &[], policy)?;
    record
        .gaps
        .push(json!({"route":"recorded_untrusted_gap","reason":reason,
        "status":if reason == "too_large" {"commit not checked: too large"}
        else {"commit not checked: missing or conflicting authority"}}));
    Ok(report(record, json!({}), false))
}
fn report(record: Review, key_map: Value, flagged: bool) -> Report {
    let status = if !record.gaps.is_empty() {
        "gap"
    } else if flagged {
        "jev_flag"
    } else {
        "jev_checked"
    };
    Report {
        domain: "untrusted".into(),
        record,
        key_map,
        flagged,
        native_wake_count: 0,
        status: status.into(),
    }
}
pub fn oversized(requests: &[WireRequest], calibration: Option<&Calibration>) -> Result<bool> {
    super::option1::selection::needed(requests, calibration)
}
pub fn notice(report: &Report, event: &str) -> Result<Value> {
    let mut output = json!({});
    if report.flagged {
        let maximum = report
            .record
            .maximum
            .as_ref()
            .ok_or("UNTRUSTED_FLAG_ANSWER stage=notice")?;
        let key = maximum
            .question
            .strip_prefix("rule.")
            .ok_or("UNTRUSTED_FLAG_RULE stage=notice")?;
        let source = &report.key_map["rules"][key];
        if source.is_null() {
            return Err("UNTRUSTED_FLAG_SOURCE stage=notice".into());
        }
        output["hookSpecificOutput"] = json!({"hookEventName":event,"additionalContext":format!(
            "[kah Jev flag, not confirmed by an LLM; advisory, not user steering] SHA {} rule {} source {} (Noul {}, line {}). Check the committed change against this source.",
            report.record.head, source["full_rule_id"], source["source_ids"], maximum.noul, report.record.policy.line)});
    }
    if !report.record.gaps.is_empty() {
        output["systemMessage"] = json!(format!(
            "kah Untrusted commit {} gaps: {}",
            report.record.head,
            json!(report.record.gaps)
        ));
    }
    Ok(output)
}
