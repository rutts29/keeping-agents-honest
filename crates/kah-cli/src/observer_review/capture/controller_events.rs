//! Conditional timing comes from private host delivery records, never worker prose.
use crate::Result;
use crate::observer_review::{gate_approval::followup, source_store, state::Request};
use kah_codex::delivery::{Message, Receipt};
use kah_core::run_store::RunStore;
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn add(root: &Path, request: &mut Request) -> Result<()> {
    add_at(root, request, &RunStore::default_state_root()?)
}
fn add_at(root: &Path, request: &mut Request, delivery_root: &Path) -> Result<()> {
    let mut receipts = vec![];
    let directory = root.join("gate-followups");
    if directory.is_dir() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let saved: followup::VerificationRound = source_store::read(&path)?;
            let round = followup::read(root, &saved.binding)?
                .ok_or("REVIEW_FOLLOWUP_BINDING stage=controller_events")?;
            // An accepted ask is historical evidence. A later ledger revision
            // cannot change whether it happened; its original binding stays cited.
            if round.binding.worker != request.worker
                || request.head.as_deref() != Some(&round.binding.head)
            {
                continue;
            }
            let ask = directory.join(&round.id).join("builder-request.json");
            if !ask.is_file() {
                continue;
            }
            let saved: Value = source_store::read(&ask)?;
            let text = saved["text"]
                .as_str()
                .ok_or("REVIEW_FOLLOWUP_REQUEST stage=controller_events")?;
            if saved["receipt"] != round.builder_receipt() {
                return Err("REVIEW_FOLLOWUP_BINDING stage=controller_events".into());
            }
            let message = Message::new(
                &request.worker,
                "kah-observer",
                &round.builder_receipt(),
                text,
            )?;
            if Receipt::accepted(delivery_root, &message)? {
                receipts.push(json!({"round":round.id,"binding":round.binding,"delivery_receipt":round.builder_receipt()}));
            }
        }
    }
    request.documents.push(Document::new(
        "controller-evidence",
        "controller_event",
        "kah:controller-evidence",
        &json!({
            "worker":request.worker,"request":request.id,"head":request.head,
            "event":"observer_evidence_ask","occurred":!receipts.is_empty(),"receipts":receipts
        })
        .to_string(),
    ));
    Ok(())
}

pub fn bind(request: &Request, report: &kah_observer::review_evidence::Report) -> Result<()> {
    for requirement in report
        .decision_units
        .as_ref()
        .into_iter()
        .flat_map(|u| &u.requirements)
    {
        if let Some(trigger) = requirement
            .applicability
            .as_ref()
            .and_then(|a| a.controller_trigger.as_ref())
            && (trigger.receipt.worker != request.worker
                || trigger.receipt.request != request.id
                || trigger.receipt.head != request.head)
        {
            return Err("REVIEW_TRIGGER_BINDING stage=report: conditional receipt belongs to another worker, request or SHA".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/historical_evidence_ask.rs"]
mod tests;
