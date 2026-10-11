use super::*;
pub(super) fn validate<'a>(
    catalogue: &Catalogue,
    manifest: &'a Manifest,
    known: &BTreeMap<&str, &Record>,
) -> Result<BTreeSet<&'a str>, String> {
    if manifest.body_digest != catalogue.body_digest
        || manifest.catalogue_digest != catalogue_digest(catalogue)
    {
        return Err("PREPARATION_BODY_BINDING stage=whole_preparation".into());
    }
    let selected = manifest
        .selected
        .iter()
        .map(|c| c.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for (include, choices) in [(true, &manifest.selected), (false, &manifest.excluded)] {
        for choice in choices {
            let record = known
                .get(choice.id.as_str())
                .ok_or("PREPARATION_UNKNOWN_RECORD stage=whole_preparation")?;
            if !seen.insert(choice.id.as_str())
                || choice.digest != record.digest
                || choice.reason.trim().is_empty()
            {
                return Err("PREPARATION_ACCOUNTING stage=whole_preparation".into());
            }
            if !include && record.required {
                return Err("PREPARATION_REQUIRED_RECORD stage=whole_preparation".into());
            }
        }
    }
    if seen.len() != known.len() {
        return Err("PREPARATION_UNACCOUNTED_RECORD stage=whole_preparation".into());
    }
    for id in &selected {
        if known[id]
            .dependencies
            .iter()
            .any(|d| !selected.contains(d.as_str()))
        {
            return Err("PREPARATION_DEPENDENCY stage=whole_preparation".into());
        }
    }
    Ok(selected)
}
