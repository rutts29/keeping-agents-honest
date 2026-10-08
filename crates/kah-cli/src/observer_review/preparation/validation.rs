use super::*;

pub(crate) fn validate_selection(job: &Job, bytes: &[u8]) -> Result<Manifest> {
    super::super::failure::validation(super::super::failure::Origin::SelectionValidation, || {
        let manifest: Manifest = serde_json::from_slice(bytes)
            .map_err(|e| format!("CONTEXT_MANIFEST_JSON stage=context_selection: {e}"))?;
        context_selection::validate(&job.decision, &job.items, &manifest, &BTreeMap::new())?;
        Ok(manifest)
    })
}
