//! Visible uncertainty is retained separately from confirmed corrective findings.
use super::state::{Request, Store};
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
mod identity;
mod rethink;
pub use identity::fresh_followup;
pub use rethink::{resume, retain_rethink, rethink, schedule};

pub fn notice(request: &Request, report: &Report, assessment: &Value) -> Option<String> {
    if assessment["state"] == "checkpoint_unconfirmed_observation" {
        return Some(format!(
            "kah observer retains an unconfirmed status observation for {}. Status interpretation stays with the observer: inspect existing evidence, ask the builder for evidence, then propose or run objective checks. This is not a user decision or a gate verdict; unrelated work and observation continue.",
            request.id
        ));
    }
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
    if super::commit_packet::is_commit(request) {
        return super::light_commit::record(store, request, report, assessment, judgment);
    }
    let followup_finished =
        super::gate_approval::finish_followup(&store.directory, request, report, assessment)?;
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
    let fresh = followup_finished || fresh_followup(&store.directory, request, report, assessment)?;
    if !followup_finished
        && fresh
        && !super::gate_approval::begin_followup(store, request, report, assessment)?
    {
        retain_rethink(&store.directory, request, assessment)?;
    }
    if let Some(mut text) = notice(request, report, assessment) {
        if !fresh {
            text.push_str(" The same SHA, unresolved items and checked evidence already received a follow-up; no new automatic review cycle is started. Waiting for changed authority, SHA or evidence.");
        }
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
