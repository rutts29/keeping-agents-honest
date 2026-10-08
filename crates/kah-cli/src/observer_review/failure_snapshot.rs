//! Bind detached validation diagnostics to the exact immutable inputs they checked.
use super::{
    failure::{self, Origin},
    state::Store,
};
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) mod gap;
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn diagnostic(error: &dyn std::fmt::Display) -> String {
    let text = error.to_string();
    if text.contains(" stage=") {
        text
    } else {
        format!("REVIEW_IO stage=review: {text}")
    }
}
pub(crate) fn capture(worker: &str, error: &(dyn std::error::Error + 'static)) -> Result<Value> {
    let origin = failure::origin(error);
    let mut result = json!({"origin":origin,"error":diagnostic(&error)});
    if let Some(error) = error.downcast_ref::<super::consume::supervision_failure::Failure>() {
        result["request"] = json!(error.request);
        result["child_pid"] = error.child_pid.into();
        return Ok(result);
    }
    if !matches!(
        origin,
        Origin::ReportValidation | Origin::SelectionValidation
    ) {
        result["request"] = json!(
            Store::read(worker)
                .ok()
                .flatten()
                .and_then(|s| s.pending)
                .map(|p| p.id)
        );
        return Ok(result);
    }
    let Some(state) = Store::read(worker)? else {
        return Ok(result);
    };
    let Some(pending) = &state.pending else {
        return Ok(result);
    };
    let root = Store::directory(worker)?;
    let (input, path) = if origin == Origin::SelectionValidation {
        let decision = super::preparation::active_decision(worker, &pending.id)?;
        let input: Value =
            super::source_store::read(&root.join(format!("preparation-{decision}.json")))?;
        (
            input,
            state
                .observer
                .workspace
                .join("outbox")
                .join(format!("selected-{decision}.json")),
        )
    } else {
        (
            serde_json::to_value(super::workspace::request(
                &Store::open_wait(worker)?,
                &pending.id,
            )?)?,
            state
                .observer
                .workspace
                .join("outbox")
                .join(format!("submitted-{}.json", pending.id)),
        )
    };
    let bytes = super::consume::read_report(&path)?;
    result["request"] = pending.id.clone().into();
    result["request_digest"] = digest(&serde_json::to_vec(&input)?).into();
    result["report_digest"] = digest(&bytes).into();
    result["report"] = path.to_string_lossy().into_owned().into();
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/observer_unit/post_ack_validation.rs"]
mod tests;

/// A structurally ACKed report can fail decision-dependent validation later.
/// Do not repeat the judgment of those exact failed inputs while correcting.
pub(super) fn unchanged(
    root: &std::path::Path,
    request: &super::state::Request,
    report: &std::path::Path,
) -> Result<bool> {
    let Some(gap) = super::state::read::<Value>(&root.join("gap.json"))? else {
        return Ok(false);
    };
    let origin = serde_json::from_value(gap["origin"].clone()).unwrap_or_default();
    if !failure::correctable(origin, gap["error"].as_str().unwrap_or(""))
        || gap["request"] != request.id
        || gap["request_digest"] != digest(&serde_json::to_vec(request)?)
    {
        return Ok(false);
    }
    Ok(gap["report_digest"] == digest(&super::consume::read_report(report)?))
}
