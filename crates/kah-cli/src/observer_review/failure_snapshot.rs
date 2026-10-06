//! Bind detached validation diagnostics to the exact immutable inputs they checked.
use super::{
    failure::{self, Origin},
    state::Store,
};
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn capture(worker: &str, error: &(dyn std::error::Error + 'static)) -> Result<Value> {
    let origin = failure::origin(error);
    let mut result = json!({"origin":origin,"error":error.to_string()});
    if !matches!(
        origin,
        Origin::ReportValidation | Origin::SelectionValidation
    ) {
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
