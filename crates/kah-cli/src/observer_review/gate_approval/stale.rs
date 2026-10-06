//! Changed host scope retires the old review; it is not a thinker correction.
use super::super::state::{State, Store};
use super::*;

pub fn release(store: &Store, state: &mut State, request: &Request) -> Result<bool> {
    let Some(id) = request.trigger["verification_followup"]["round"].as_str() else {
        return Ok(false);
    };
    if !id.starts_with("followup-") || !id[9..].bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("REVIEW_FOLLOWUP_BINDING stage=stale_review".into());
    }
    let saved: followup::VerificationRound = super::super::source_store::read(
        &store
            .directory
            .join("gate-followups")
            .join(format!("{id}.json")),
    )?;
    let round = followup::read(&store.directory, &saved.binding)?
        .ok_or("REVIEW_FOLLOWUP_BINDING stage=stale_review")?;
    if round.id != id || round.binding.worker != request.worker {
        return Err("REVIEW_FOLLOWUP_BINDING stage=stale_review".into());
    }
    if request.head.as_deref() == Some(&round.binding.head)
        && request.intent["revision"].as_u64() == Some(round.binding.intent_revision)
    {
        return Ok(false);
    }
    if state.pending.as_ref().is_none_or(|p| p.id != request.id) {
        return Err("REVIEW_REQUEST_MISMATCH stage=stale_review".into());
    }
    let directory = store.directory.join("obsolete-reviews");
    kah_core::run_store::ensure_private_directory(&directory)?;
    super::super::source_store::save(
        &directory,
        &format!("{}.json", request.id),
        &json!({
            "request":request,"round":round.binding,"state":"scope_changed","passed":null,
            "reason":"HEAD or user intent changed; original verification evidence retained; no user risk decision on stale scope"
        }),
    )?;
    let text = format!(
        "kah verification review {} is obsolete: its round {} belongs to SHA {} and intent revision {}; the current HEAD or intent changed. Original evidence is retained; later queued work continues.",
        request.id, round.id, round.binding.head, round.binding.intent_revision
    );
    super::super::reminder_queue::save(
        &store.directory,
        &json!({"request":request.id,"purpose":"uncertainty","turn":request.turn,"system_notice":true,"text":text}),
    )?;
    state.pending = None;
    store.save(state)?;
    Ok(true)
}
