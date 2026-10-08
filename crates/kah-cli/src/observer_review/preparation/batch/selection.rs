use super::*;
use crate::observer_review::{source_store, state::Store};
pub fn accept(worker: &str, batch: &Batch) -> Result<bool> {
    let state = crate::observer_review::state::read_control(worker)?
        .ok_or("PREPARATION_BATCH_STATE stage=context_preparation")?;
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selected-{}.json", batch.id));
    if !path.exists() {
        return Ok(false);
    }
    let input = crate::observer_review::consume::read_report(&path)?;
    let (_, manifests) = validate_submission_iter(
        batch,
        batch.decisions.iter().map(|entry| load_job(worker, entry)),
        &input,
    )?;
    for manifest in manifests {
        source_store::save(
            &Store::directory(worker)?,
            &format!("selection-{}.json", manifest.decision),
            &serde_json::to_value(manifest)?,
        )?;
    }
    Ok(true)
}
