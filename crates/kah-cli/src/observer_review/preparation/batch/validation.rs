use super::*;
/// Hold manifests and one original job, never K complete decision states.
pub fn validate_iter(
    batch: &Batch,
    jobs: impl IntoIterator<Item = Result<Job>>,
    manifests: &[Manifest],
) -> Result<Vec<Manifest>> {
    crate::observer_review::failure::validation(
        crate::observer_review::failure::Origin::SelectionValidation,
        || {
            let mut by_id = BTreeMap::new();
            for manifest in manifests {
                if by_id.insert(&manifest.decision, manifest).is_some() {
                    return Err(format!(
                        "CONTEXT_BATCH_DUPLICATE stage=context_selection: {}",
                        manifest.decision
                    )
                    .into());
                }
            }
            let mut entries = vec![];
            let mut checked = vec![];
            for job in jobs {
                let job = job?;
                if job.worker != batch.worker {
                    return Err(
                        "PREPARATION_BATCH_CHANGED stage=context_preparation: worker".into(),
                    );
                }
                entries.push(Entry {
                    decision: job.decision.clone(),
                    digest: digest(&job)?,
                });
                let manifest = by_id.remove(&job.decision).ok_or_else(|| {
                    format!(
                        "CONTEXT_BATCH_MISSING stage=context_selection: {}",
                        job.decision
                    )
                })?;
                checked.push(super::super::validate_selection(
                    &job,
                    &serde_json::to_vec(manifest)?,
                )?);
            }
            let expected = Batch::from_entries(&batch.review, &batch.worker, entries)?;
            if expected.id != batch.id {
                return Err("PREPARATION_BATCH_CHANGED stage=context_preparation".into());
            }
            if !by_id.is_empty() {
                return Err(format!(
                    "CONTEXT_BATCH_UNKNOWN stage=context_selection: {:?}",
                    by_id.keys()
                )
                .into());
            }
            Ok(checked)
        },
    )
}
