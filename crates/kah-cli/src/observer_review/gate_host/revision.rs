//! Revised reports retain their old host job and resume without native redelivery.
use super::*;
/// A saved job was created only after its own ACK, before yielding the slot.
/// A later normal request can overwrite the global report-ready marker.
pub(super) fn owns_ack(state: &State, request: &Request, submitted: &Path) -> Result<bool> {
    let root = directory(&request.worker, &request.id)?;
    if root.join("finished.json").exists() {
        return Ok(false);
    }
    let path = root.join("job.json");
    let Some(old) = super::super::state::read::<Job>(&path)? else {
        return Ok(false);
    };
    Ok(state.worker.session == request.worker
        && old.pending.id == request.id
        && serde_json::to_value(&old.request)? == serde_json::to_value(request)?
        && state.observer.session.is_some()
        && old.state.observer.session == state.observer.session
        && old.state.observer.workspace == state.observer.workspace
        && old.state.observer.pane == state.observer.pane
        && old.submitted == submitted)
}
pub(super) fn key(job: &Job) -> Result<String> {
    Ok(match &job.report_revision {
        Some(revision) => revision.digest.clone(),
        None => format!(
            "legacy-{}",
            super::super::report_revision::digest(&serde_json::to_vec(&job.report)?)
        ),
    })
}
pub(super) fn require_current(job: &Job) -> Result<()> {
    if let Some(revision) = &job.report_revision {
        return revision.require_current();
    }
    // Historical jobs did not retain raw bytes: compare parsed content, labelled
    // legacy. Never invent an exact-byte digest for the overwritten original.
    let current =
        serde_json::from_slice::<Report>(&super::super::consume::read_report(&job.submitted)?).ok();
    if current.map(serde_json::to_value).transpose()? != Some(serde_json::to_value(&job.report)?) {
        return Err(
            "REVIEW_REPORT_REVISION_SUPERSEDED stage=handoff: legacy parsed report superseded"
                .into(),
        );
    }
    Ok(())
}
pub(super) fn changed(old: &Job, new: &Job) -> Result<bool> {
    Ok(
        serde_json::to_value(&old.report)? != serde_json::to_value(&new.report)?
            || old
                .report_revision
                .as_ref()
                .zip(new.report_revision.as_ref())
                .is_some_and(|(a, b)| a.digest != b.digest),
    )
}
pub(super) fn archive(root: &Path, old: &Job) -> Result<()> {
    let archive = root.join("revisions").join(key(old)?);
    kah_core::run_store::ensure_private_directory(&archive)?;
    source_store::save(&archive, "job.json", &serde_json::to_value(old)?)?;
    for name in ["evaluated.json", "stage.json"] {
        let original = root.join(name);
        if original.is_file() {
            fs::rename(original, archive.join(name))?;
        }
    }
    super::super::consume::outcome::record(
        &old.request.worker,
        json!({"kind":"report_revision_superseded","request":old.request.id,"revision":key(old)?,"retained":archive}),
    )
}
pub(super) fn resume_latest(job: &Job) -> Result<()> {
    let store = Store::open_wait(&job.request.worker)?;
    let mut state = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=report_revision")?;
    if state.observer.session != job.state.observer.session {
        return Err("REVIEW_GATE_HOST_OBSERVER stage=report_revision".into());
    }
    if state.pending.is_none() {
        state.pending = Some(job.pending.clone());
        store.save(&state)?;
    }
    marker(
        &directory(&job.request.worker, &job.request.id)?,
        "stage.json",
        "evaluating",
    )?;
    super::super::consume::outcome::record(
        &job.request.worker,
        json!({"kind":"report_revision_superseded","request":job.request.id,"revision":key(job)?,"resume":"newest submitted bytes; no native redelivery"}),
    )?;
    super::super::hook::spawn("__review-consume", &job.request.worker)
}
