//! A failed builder notice remains durable without owning the review slot.
use super::followup::VerificationRound;
use super::*;

pub(super) fn deliver(
    root: &Path,
    request: &Request,
    round: &VerificationRound,
    items: &[Value],
) -> Result<()> {
    deliver_with(root, request, round, items, |text, receipt| {
        crate::send_cli::run(&[
            "send".into(),
            request.worker.clone(),
            text.into(),
            "--sender".into(),
            "kah-observer".into(),
            "--receipt".into(),
            receipt.into(),
        ])
    })
}
fn deliver_with(
    root: &Path,
    request: &Request,
    round: &VerificationRound,
    items: &[Value],
    mut send: impl FnMut(&str, &str) -> Result<()>,
) -> Result<()> {
    let directory = root
        .join("gate-followups")
        .join(&round.id)
        .join("send-back");
    kah_core::run_store::ensure_private_directory(&directory)?;
    for item in items {
        let id = item["id"]
            .as_str()
            .ok_or("REVIEW_CONTRACT_ITEM stage=delivery")?;
        let record = format!("{id}.json");
        let prior: Option<Value> = super::super::state::read(&directory.join(&record))?;
        if prior.as_ref().is_some_and(|p| p["submitted"] == true) {
            continue;
        }
        let receipt = format!("{}-needs-check-{id}", round.id);
        let text = format!(
            "kah item {id} needs a check covering {} on SHA {}. The gate is not passed. This is worker verification work. Attempts and receipts: {}.",
            item["claim"],
            round.binding.head,
            directory.parent().unwrap().display()
        );
        let delivered = send(&text, &receipt);
        super::super::source_store::save(
            &directory,
            &record,
            &json!({"binding":round.binding,"item":item,"receipt":receipt,"submitted":delivered.is_ok(),"error":delivered.as_ref().err().map(ToString::to_string)}),
        )?;
        let text = if let Err(error) = delivered {
            format!("{text} Builder notice delivery failed: {error}; durable retry retained.")
        } else {
            text
        };
        super::super::reminder_queue::save(
            root,
            &json!({"request":request.id,"purpose":"checkpoint","turn":request.turn,"system_notice":true,"text":text}),
        )?;
    }
    Ok(())
}

pub(super) fn retry(root: &Path, request: &Request, round: &VerificationRound) -> Result<()> {
    let path = root
        .join("gate-followups")
        .join(&round.id)
        .join("finished.json");
    let Some(saved) = super::super::state::read::<Value>(&path)? else {
        return Ok(());
    };
    if saved["status"] != "needs_check" {
        return Ok(());
    }
    let items = saved["items"]
        .as_array()
        .ok_or("REVIEW_CONTRACT_ITEMS stage=delivery")?;
    deliver(root, request, round, items)
}
#[cfg(test)]
#[path = "../../../tests/observer_unit/contract_delivery.rs"]
mod tests;
