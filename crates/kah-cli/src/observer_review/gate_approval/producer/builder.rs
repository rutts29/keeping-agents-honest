use super::*;
use sha2::{Digest, Sha256};

pub(super) fn attempt(
    root: &Path,
    state: &super::super::super::state::State,
    round: &VerificationRound,
    request: &Request,
    job: &Job,
) -> Result<Option<BuilderReceipt>> {
    let directory = directory(root, round);
    let text = query(round, job);
    let pointer = EvidencePointer {
        receipt: round.builder_receipt(),
        record: directory.join("builder-request.json").display().to_string(),
        digest: format!("sha256:{:x}", Sha256::digest(text.as_bytes())),
    };
    if claim(root, &round.binding, Stage::Builder)? {
        source_store::save(
            &directory,
            "builder-request.json",
            &json!({"text":text,"receipt":pointer.receipt,
            "created_ms":kah_observer::session::now_ms(),"transcript":request.documents.iter().find(|d|d.id=="transcript")}),
        )?;
        // Herdr v0.9.3 native agent prompt, via the existing idempotent delegate
        // channel. Submission acknowledgement is not a builder answer.
        if let Err(error) = crate::send_cli::run(&[
            "send".into(),
            request.worker.clone(),
            text,
            "--sender".into(),
            "kah-observer".into(),
            "--receipt".into(),
            pointer.receipt.clone(),
        ]) {
            source_store::save(
                &directory,
                "builder-delivery-error.json",
                &json!({"error":error.to_string()}),
            )?;
            return Ok(Some(BuilderReceipt {
                request: pointer,
                response: BuilderOutcome::Unavailable {
                    reason: error.to_string(),
                },
            }));
        }
    }
    read_response(root, state, round, pointer)
}
fn read_response(
    root: &Path,
    state: &super::super::super::state::State,
    round: &VerificationRound,
    pointer: EvidencePointer,
) -> Result<Option<BuilderReceipt>> {
    let directory = directory(root, round);
    let saved: Option<Value> =
        super::super::super::state::read(&directory.join("builder-request.json"))?;
    let Some(saved) = saved else {
        return Err("REVIEW_FOLLOWUP_DELIVERY_UNKNOWN stage=verification: operation reserved before a crash; request retained, do not send twice".into());
    };
    let before: kah_observer::review_evidence::Document =
        serde_json::from_value(saved["transcript"].clone())?;
    let home =
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=verification")?);
    // Reuse the existing native history completion contract (thread turn
    // status / Claude durable turn records), not TUI text or a new deadline.
    let mut history = crate::native_history::History::open(
        &home,
        root,
        &state.worker.provider,
        &state.worker.project,
        &state.worker.session,
    )?;
    if !history.completed()? {
        return Ok(None);
    }
    let current = super::super::super::capture::history_document(
        &mut history,
        &state.worker.provider,
        &state.worker.session,
    )?;
    if let Some(record) = response(&before, &current, &round.builder_receipt()) {
        let text = &current.text[record.start_byte..record.end_byte];
        let response = EvidencePointer {
            receipt: format!("{}-response", round.id),
            record: directory
                .join("builder-response.json")
                .display()
                .to_string(),
            digest: format!("sha256:{:x}", Sha256::digest(text.as_bytes())),
        };
        source_store::save(
            &directory,
            "builder-response.json",
            &json!({"source":current.uri,"record":record.context,"text":text,"digest":response.digest}),
        )?;
        return Ok(Some(BuilderReceipt {
            request: pointer,
            response: BuilderOutcome::Evidence { response },
        }));
    }
    if let Some(record) = new_primary_reply(&before, &current) {
        return unmatched(&directory, &current, record, pointer);
    }
    Ok(None)
}
fn query(round: &VerificationRound, job: &Job) -> String {
    let ids = job.assessment["needs_thinker"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "kah evidence request {} for checkpoint {} on SHA {}: provide specific source/check receipts for unresolved items {ids}. Reply with this request receipt and the evidence. Your answer is evidence, not user authority; do not change authorized scope or implement unrelated work. The observer will independently check the same SHA before deciding what remains unverified.",
        round.builder_receipt(),
        round.binding.review,
        round.binding.head
    )
}
fn response<'a>(
    before: &kah_observer::review_evidence::Document,
    current: &'a kah_observer::review_evidence::Document,
    receipt: &str,
) -> Option<&'a kah_observer::review_evidence::NativeRecord> {
    current.native_records.iter().find(|record| {
        is_new_primary(record, before)
            && current
                .text
                .get(record.start_byte..record.end_byte)
                .is_some_and(|text| text.contains(receipt))
    })
}
fn new_primary_reply<'a>(
    before: &kah_observer::review_evidence::Document,
    current: &'a kah_observer::review_evidence::Document,
) -> Option<&'a kah_observer::review_evidence::NativeRecord> {
    current
        .native_records
        .iter()
        .rev()
        .find(|record| is_new_primary(record, before))
}
fn is_new_primary(
    record: &kah_observer::review_evidence::NativeRecord,
    before: &kah_observer::review_evidence::Document,
) -> bool {
    let context = &record.context;
    (context["record_type"] == "agentMessage" || context["message_role"] == "assistant")
        && context["is_meta"] != true
        && context["is_sidechain"] != true
        && !before
            .native_records
            .iter()
            .any(|old| old.context["record_id"] == context["record_id"])
}

#[cfg(test)]
#[path = "../../../../tests/observer_unit/builder_evidence.rs"]
mod tests;

fn unmatched(
    directory: &Path,
    current: &kah_observer::review_evidence::Document,
    record: &kah_observer::review_evidence::NativeRecord,
    pointer: EvidencePointer,
) -> Result<Option<BuilderReceipt>> {
    let path = directory.join("builder-unmatched-response.json");
    source_store::save(
        directory,
        "builder-unmatched-response.json",
        &json!({"source":current.uri,"record":record.context,"text":current.text.get(record.start_byte..record.end_byte)}),
    )?;
    Ok(Some(BuilderReceipt {
        request: pointer,
        response: BuilderOutcome::Unavailable {
            reason: format!(
                "Completed native reply omitted the requested receipt; full evidence retained at {}. No second ask.",
                path.display()
            ),
        },
    }))
}
