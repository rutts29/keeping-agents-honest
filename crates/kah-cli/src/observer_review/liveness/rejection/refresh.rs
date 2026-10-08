//! A changed observer report needs fresh host validation, never a stale digest.
use super::*;

pub(super) fn changed(outbox: &Path, request: &Request) -> Result<()> {
    let bytes = fs::read(outbox.join(format!("rejected-{}.json", request.id)))?;
    let Ok(prior) = serde_json::from_slice::<Rejection>(&bytes) else {
        return Ok(());
    };
    if prior.request_digest != digest(&serde_json::to_vec(request)?)
        || prior
            .report
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Ok(());
    }
    let root = outbox
        .parent()
        .ok_or("REVIEW_REPORT_PATH stage=report_refresh")?
        .canonicalize()?;
    let Ok(path) = root.join(&prior.report).canonicalize() else {
        return Ok(());
    };
    if !path.starts_with(&root) {
        return Ok(());
    }
    let current = crate::observer_review::commit_packet::read_bound_report(&root, &path)?;
    if digest(&current) == prior.report_digest {
        return Ok(());
    }
    // Revalidation only diagnoses rejected content. A valid edited file still
    // needs the observer's normal foreground report command and ACK.
    let Err(error) = workspace::validate_report_bytes(request, &current) else {
        return Ok(());
    };
    let archived = serde_json::to_value(&prior)?;
    workspace::publish_output(
        outbox,
        &format!("rejection-{}.json", digest(&serde_json::to_vec(&archived)?)),
        &archived,
    )?;
    record(outbox, request, &path, &current, error.as_ref())
}
