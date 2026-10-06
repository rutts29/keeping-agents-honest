//! Derive selection failure origin from host validation, never shared metadata.
use super::super::{
    failure::Origin,
    preparation::{self, Job},
    state::{self, Store},
    workspace,
};
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn record(
    outbox: &Path,
    job: &Job,
    path: &Path,
    bytes: &[u8],
    error: &dyn std::fmt::Display,
) -> Result<()> {
    let root = outbox
        .parent()
        .ok_or("PREPARATION_PATH stage=selection_rejection")?
        .canonicalize()?;
    let report = path
        .canonicalize()?
        .strip_prefix(&root)
        .map_err(|_| "PREPARATION_PATH stage=selection_rejection")?
        .to_path_buf();
    workspace::publish_output(
        outbox,
        &format!("rejected-selection-{}.json", job.decision),
        &json!({
            "decision":job.decision,"request_digest":digest(&serde_json::to_vec(job)?),
            "report_digest":digest(bytes),"report":report,"error":error.to_string()
        }),
    )
}
pub(crate) fn current(
    root: &Path,
    outbox: &Path,
    worker: &str,
    request: &str,
) -> Result<Option<String>> {
    let decision = preparation::active_decision(worker, request)?;
    let Some(record) =
        state::read::<Value>(&outbox.join(format!("rejected-selection-{decision}.json")))?
    else {
        return Ok(None);
    };
    let job: Job =
        super::super::source_store::read(&root.join(format!("preparation-{decision}.json")))?;
    if record["request_digest"] != digest(&serde_json::to_vec(&job)?) {
        return Ok(None);
    }
    let base = outbox
        .parent()
        .ok_or("PREPARATION_PATH stage=selection_rejection")?
        .canonicalize()?;
    let path = base
        .join(
            record["report"]
                .as_str()
                .ok_or("PREPARATION_PATH stage=selection_rejection")?,
        )
        .canonicalize()?;
    if !path.starts_with(&base) {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    if record["report_digest"] != digest(&bytes) {
        return Ok(None);
    }
    let Err(error) = preparation::validate_selection(&job, &bytes) else {
        return Ok(None);
    };
    if record["error"] != error.to_string() {
        return Ok(None);
    }
    let mut checked = record;
    checked["request"] = request.into();
    checked["origin"] = serde_json::to_value(Origin::SelectionValidation)?;
    checked["report"] = path.to_string_lossy().into_owned().into();
    crate::unattended_report::save(
        &Store::directory(worker)?,
        "validation-current.json",
        &checked,
    )?;
    Ok(Some(format!("VALIDATION BLOCKED: {}", error)))
}
