//! Shared rejection records are evidence: reproduce validation against host request.
use super::super::{state::Request, workspace};
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rejection {
    request_digest: String,
    report_digest: String,
    report: PathBuf,
    reason: String,
    #[serde(default)]
    origin: super::super::failure::Origin,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn record(
    outbox: &Path,
    request: &Request,
    report: &Path,
    bytes: &[u8],
    error: &(dyn std::error::Error + 'static),
) -> Result<()> {
    let root = outbox
        .parent()
        .ok_or("REVIEW_REPORT_PATH stage=report")?
        .canonicalize()?;
    let report = report
        .canonicalize()?
        .strip_prefix(&root)
        .map_err(|_| "REVIEW_REPORT_PATH stage=report: rejected report outside observer workspace")?
        .to_path_buf();
    let rejected = Rejection {
        request_digest: digest(&serde_json::to_vec(request)?),
        report_digest: digest(bytes),
        report,
        reason: error.to_string(),
        origin: super::super::failure::origin(error),
    };
    workspace::publish_output(
        outbox,
        &format!("rejected-{}.json", request.id),
        &serde_json::to_value(rejected)?,
    )
}
pub(crate) fn verified(outbox: &Path, request: &Request) -> Result<Option<String>> {
    if fs::read(outbox.join(format!("submitted-{}.json", request.id)))
        .ok()
        .is_some_and(|bytes| workspace::validate_report_bytes(request, &bytes).is_ok())
    {
        return Ok(None);
    }
    let path = outbox.join(format!("rejected-{}.json", request.id));
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let Ok(record) = serde_json::from_slice::<Rejection>(&bytes) else {
        return Ok(None);
    };
    if record.request_digest != digest(&serde_json::to_vec(request)?)
        || record
            .report
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Ok(None);
    }
    let root = outbox
        .parent()
        .ok_or("REVIEW_REPORT_PATH stage=liveness")?
        .canonicalize()?;
    let Ok(report_path) = root.join(&record.report).canonicalize() else {
        return Ok(None);
    };
    if !report_path.starts_with(&root) {
        return Ok(None);
    }
    let report = fs::read(report_path)?;
    if digest(&report) != record.report_digest {
        return Ok(None);
    }
    Ok(workspace::validate_report_bytes(request, &report)
        .err()
        .filter(|e| e.to_string() == record.reason)
        .map(|_| format!("VALIDATION BLOCKED: {}", record.reason)))
}
pub(crate) fn current(
    root: &Path,
    outbox: &Path,
    worker: &str,
    id: &str,
) -> Result<Option<String>> {
    if !outbox.join(format!("rejected-{id}.json")).exists() {
        return Ok(None);
    }
    let request: Request =
        super::super::source_store::read(&root.join("requests").join(format!("{id}.json")))?;
    if request.worker != worker || request.id != id {
        return Ok(None);
    }
    let status = verified(outbox, &request)?;
    if status.is_some() {
        let record: Rejection =
            serde_json::from_slice(&fs::read(outbox.join(format!("rejected-{id}.json")))?)?;
        let bytes = fs::read(outbox.parent().unwrap().join(&record.report))?;
        let error = workspace::validate_report_bytes(&request, &bytes).unwrap_err();
        let origin = super::super::failure::origin(error.as_ref());
        crate::unattended_report::save(
            root,
            "validation-current.json",
            &serde_json::json!({
                "request":id,"request_digest":record.request_digest,"report_digest":record.report_digest,
                "error":record.reason,"origin":origin,"report":outbox.parent().unwrap().join(record.report)
            }),
        )?;
    }
    Ok(status)
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/rejection_status.rs"]
mod tests;
