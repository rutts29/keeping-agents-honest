//! Visible uncertainty is retained separately from confirmed corrective findings.
use super::state::{Request, Store};
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
mod rethink;
pub use rethink::{resume, retain_rethink, rethink, schedule};

pub fn notice(request: &Request, report: &Report, assessment: &Value) -> Option<String> {
    let ids = assessment["needs_user"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(assessment["needs_thinker"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return None;
    }
    let names = ids
        .iter()
        .map(|id| {
            report
                .decision_units
                .as_ref()
                .and_then(|u| u.requirements.iter().find(|r| r.id == *id))
                .map(|r| format!("{} ({id})", r.name))
                .unwrap_or_else(|| (*id).to_owned())
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "kah observer is still unsure about {names}. Review {} retains the evidence and reasoning. Only the affected decision waits. Unrelated authorized work can continue.",
        request.id
    ))
}
pub fn record(
    store: &Store,
    request: &Request,
    report: &Report,
    assessment: &Value,
    judgment: &kah_observer::judge::Judgment,
) -> Result<()> {
    super::questions::resolve(&store.directory, request, report, judgment)?;
    super::questions::store(&store.directory, request, report, assessment)?;
    if let Some(proof) = super::gate_approval::captured(request)?
        && let Some(question) = super::gate_approval::store(
            &store.directory,
            request,
            report,
            assessment,
            &proof,
            &serde_json::to_value(judgment)?,
        )?
    {
        super::reminder_queue::save(
            &store.directory,
            &json!({"request":request.id,"purpose":"uncertainty","turn":request.turn,
                "system_notice":true,"text":format!("kah gate {} waits on user question {}. No confirmed violation; exact-SHA delivery receipts pass. Neither passed nor failed. Unrelated authorized work continues.",request.id,question["id"])}),
        )?;
        return Ok(());
    }
    retain_rethink(&store.directory, request, assessment)?;
    if let Some(mut text) = notice(request, report, assessment) {
        text.push_str(&format!(
            " Full record: {}",
            store
                .directory
                .join("results")
                .join(format!("{}.json", request.id))
                .display()
        ));
        super::reminder_queue::save(
            &store.directory,
            &json!({"request":request.id,"purpose":"uncertainty","turn":request.turn,"text":text,"system_notice":true}),
        )?;
    }
    Ok(())
}
