use super::*;
use crate::observer_review::{source_store, state::Store};
pub fn accept(worker: &str, batch: &Batch) -> Result<bool> {
    let state = Store::read(worker)?.ok_or("PREPARATION_BATCH_STATE stage=context_preparation")?;
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selected-{}.json", batch.id));
    if !path.exists() {
        return Ok(false);
    }
    let input = crate::observer_review::consume::read_report(&path)?;
    let (_, manifests) = validate_submission(batch, &jobs(worker, batch)?, &input)?;
    for manifest in manifests {
        source_store::save(
            &Store::directory(worker)?,
            &format!("selection-{}.json", manifest.decision),
            &serde_json::to_value(manifest)?,
        )?;
    }
    Ok(true)
}
