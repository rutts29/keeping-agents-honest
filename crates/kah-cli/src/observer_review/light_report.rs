//! B' transport only: code expands bookkeeping, the thinker supplies decisions.
use super::state::Request;
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::Value;
mod model;
pub mod schema;
pub fn decode(request: &Request, bytes: &[u8]) -> Result<Report> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("REVIEW_REPORT_JSON stage=report: {e}"))?;
    if value.get("format").is_none() {
        return serde_json::from_value(value)
            .map_err(|e| format!("REVIEW_REPORT_JSON stage=report: {e}").into());
    }
    if !super::commit_packet::is_commit(request) {
        return Err(
            "REVIEW_LIGHT_CLASS stage=report: compact reports belong to commit reviews".into(),
        );
    }
    let compact: model::Compact = serde_json::from_value(value)
        .map_err(|e| format!("REVIEW_LIGHT_JSON stage=report: {e}"))?;
    compact.expand(super::workspace::instructions_version(request)?)
}
