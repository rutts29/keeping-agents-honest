use super::super::failure::{Origin, validation};
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
/// `kah observe report`: an Option1 job is validated from selected parts and
/// never expanded whole; every other request keeps the complete legacy path.
pub fn report(args: &[String]) -> Result<()> {
    let selected = args.len() == 6 && args[0] == "observe" && args[2] == "--request";
    let header = selected
        .then(|| super::report_source::header(Path::new(&args[3])))
        .transpose()
        .map_err(|_| "REVIEW_REQUEST_JSON stage=report")?;
    match header {
        Some(header) if header.option1 && args[1] == "report" && args[4] == "--json" => {
            submit_selected(Path::new(&args[3]), &header.id, Path::new(&args[5]))
        }
        _ => workspace::submit(args),
    }
}
fn submit_selected(request: &Path, id: &str, report: &Path) -> Result<()> {
    if !id.starts_with("review-")
        || id.len() > 128
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("REVIEW_REQUEST_ID stage=report".into());
    }
    let bytes = std::fs::read(report)?;
    let directory = request
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("REVIEW_REPORT_PATH stage=report")?
        .join("outbox");
    let report = validation(Origin::ReportValidation, || protocol::decode(id, &bytes))?;
    let job = super::report_source::job(request, &report)
        .map_err(|_| "REVIEW_REQUEST_JSON stage=report")?;
    validation(Origin::ReportValidation, || protocol::check(&job, &report))?;
    acknowledge(&directory, id, report)
}
/// Complete-request path, kept for callers that already hold the whole Request.
pub fn submit(directory: &Path, request: &Request, bytes: &[u8]) -> Result<()> {
    let report = validation(Origin::ReportValidation, || {
        protocol::validate(request, bytes)
    })?;
    acknowledge(directory, &request.id, report)
}
fn acknowledge(directory: &Path, id: &str, report: protocol::Report) -> Result<()> {
    workspace::publish_output(
        directory,
        &format!("submitted-{id}.json"),
        &serde_json::to_value(report)?,
    )?;
    println!(
        "{}",
        json!({"request":id,"status":"evidence_submitted","decision":"native confirmation pending host verification"})
    );
    Ok(())
}
