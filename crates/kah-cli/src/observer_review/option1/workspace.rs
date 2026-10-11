use super::super::{state::Request, workspace};
use super::protocol;
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;
pub fn schema(request: &Request, legacy: Value) -> Value {
    if protocol::active(request) {
        protocol::schema(request.trigger["option1"]["stage"].as_str().unwrap_or(""))
    } else {
        legacy
    }
}
pub fn submit(directory: &Path, request: &Request, bytes: &[u8]) -> Result<()> {
    let report =
        super::super::failure::validation(super::super::failure::Origin::ReportValidation, || {
            protocol::validate(request, bytes)
        })?;
    workspace::publish_output(
        directory,
        &format!("submitted-{}.json", request.id),
        &serde_json::to_value(report)?,
    )?;
    println!(
        "{}",
        json!({"request":request.id,"status":"evidence_submitted","decision":"native confirmation pending host verification"})
    );
    Ok(())
}
