//! A final delivery verdict owns its scope; claims are observations, not resets.
use crate::{
    Result,
    observer_review::state::{Request, State},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn preserve(state: &State, request: &Request, result: &Value) -> Result<bool> {
    let Some(c) = &state.checkpoint else {
        return Ok(false);
    };
    let binding = if c["delivery_owner"].is_object() {
        &c["delivery_owner"]
    } else {
        &c["delivery_invalidated"]
    };
    if !binding.is_object()
        || c["head"] != json!(request.head)
        || c["intent_revision"] != request.intent["revision"]
        || !routine_claim(request)
        || !result["assessment"]["violations"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        return Ok(false);
    }
    let approved = c["state"] == "checkpoint_user_approved"
        && crate::observer_review::gate_approval::approved_authority(
            state,
            request.intent["revision"].as_u64().unwrap_or(0),
        )?;
    Ok((approved || binding["authority"] == authority(request)?)
        && binding["evidence"] == evidence(request)?)
}
pub(super) fn previous_key(state: &State, request: &Request) -> Option<Value> {
    state
        .checkpoint
        .as_ref()
        .filter(|c| {
            (c["delivery_owner"].is_object() || c["delivery_invalidated"].is_object())
                && c["head"] == json!(request.head)
                && c["intent_revision"] == request.intent["revision"]
        })
        .map(|c| c["completion_key"].clone())
}
pub(super) fn bind(state: &mut State, request: &Request, previous: Option<Value>) -> Result<()> {
    let c = state
        .checkpoint
        .as_mut()
        .ok_or("REVIEW_CHECKPOINT_MISSING stage=delivery_owner")?;
    if request.trigger["verification_followup"]["stage"] != "checks_complete" {
        if let Some(key) = previous {
            c["completion_key"] = key;
            c["delivery_invalidated"] = json!({"request":request.id,"evidence":evidence(request)?,"authority":authority(request)?});
            if c["violations"].as_array().is_some_and(Vec::is_empty) {
                c["passed"] = Value::Null;
                c["state"] = "checkpoint_needs_thinker".into();
            }
        }
        return Ok(());
    }
    if c["completion_key"].is_object() {
        c["delivery_owner"] = json!({"worker":request.worker,"request":request.id,
            "authority":authority(request)?,"evidence":evidence(request)?});
    }
    Ok(())
}
fn routine_claim(request: &Request) -> bool {
    request.trigger["events"].as_array().is_some_and(|events| {
        !events.is_empty() && events.iter().all(|e| e["kind"] == "native_phase_claim")
    })
}
fn authority(request: &Request) -> Result<String> {
    digest(&json!({"goal":request.intent["goal"],"items":request.intent["items"]}))
}
fn evidence(request: &Request) -> Result<String> {
    let mut facts = vec![];
    for d in &request.documents {
        if matches!(
            d.kind.as_str(),
            "committed_code" | "plan" | "diff" | "controller_artifact_digests"
        ) || d.id == "git"
        {
            facts.push(json!({"id":d.id,"uri":d.uri,"digest":d.digest}));
        }
        if d.kind == "worker_transcript" {
            for r in &d.native_records {
                let ranges = r.context["output_ranges"].as_array();
                if ranges.is_some_and(|v| !v.is_empty()) || r.context["record_type"] == "fileChange"
                {
                    let text = d
                        .text
                        .get(r.start_byte..r.end_byte)
                        .ok_or("REVIEW_DELIVERY_EVIDENCE_RANGE stage=delivery_owner")?;
                    facts.push(json!({"record":r.context["record_id"],"kind":r.context["record_type"],"text":text}));
                }
            }
        }
    }
    digest(&json!(facts))
}
fn digest(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
