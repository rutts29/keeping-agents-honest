use super::*;

pub(super) fn resolve_sources(
    ids: &[String],
    sources: &BTreeMap<&str, (&BundleRecord, &SourceRef)>,
    referenced: &mut BTreeSet<String>,
) -> Result<Vec<SourceRef>, ValidationFailure> {
    if ids.is_empty() {
        return Err(ValidationFailure::Source);
    }
    reject_duplicates(ids, ValidationFailure::Source)?;
    ids.iter()
        .map(|id| {
            let (record, source) = sources.get(id.as_str()).ok_or(ValidationFailure::Source)?;
            if !record.is_authoritative() {
                return Err(ValidationFailure::Source);
            }
            referenced.insert(id.clone());
            Ok((*source).clone())
        })
        .collect()
}

pub(super) fn validate_new_clause(operation: &OperationProposal) -> Result<(), ValidationFailure> {
    validate_key(&operation.key)?;
    if operation.text.trim().is_empty() {
        return Err(ValidationFailure::EmptyClause);
    }
    match operation.operation {
        OperationName::Add if operation.alternatives.is_empty() => Ok(()),
        OperationName::Add => Err(ValidationFailure::AddIrrelevantFields),
        OperationName::Supersede
            if !operation.target_key.is_empty() && operation.alternatives.is_empty() =>
        {
            Ok(())
        }
        OperationName::Supersede => Err(ValidationFailure::SupersedeShape),
        OperationName::Withdraw | OperationName::Conflict => Err(ValidationFailure::EmptyClause),
    }
}

pub(super) fn insert_operation_key(
    key: &str,
    operation_keys: &mut BTreeSet<String>,
) -> Result<(), ValidationFailure> {
    validate_key(key)?;
    if operation_keys.insert(key.to_owned()) {
        Ok(())
    } else {
        Err(ValidationFailure::DuplicateOperationKey)
    }
}

pub(super) fn validate_key(key: &str) -> Result<(), ValidationFailure> {
    if key.trim().is_empty()
        || key.len() > 512
        || key.chars().any(char::is_control)
        || key != key.trim()
    {
        Err(ValidationFailure::InvalidKey)
    } else {
        Ok(())
    }
}

pub(super) fn reject_duplicates(
    values: &[String],
    failure: ValidationFailure,
) -> Result<(), ValidationFailure> {
    let mut unique = BTreeSet::new();
    if values.iter().all(|value| unique.insert(value)) {
        Ok(())
    } else {
        Err(failure)
    }
}

pub(super) fn stable_id(prefix: &str, key: &str, source_digest: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(prefix.as_bytes());
    digest.update([0]);
    digest.update(key.as_bytes());
    digest.update([0]);
    digest.update(source_digest.as_bytes());
    let hex = format!("{:x}", digest.finalize());
    format!("{prefix}-{}", &hex[..16])
}

pub(super) fn accounted_user_ids<'a>(
    bundle: &'a NeutralSessionBundle,
    accounted: &[String],
) -> Result<Vec<&'a str>, ValidationFailure> {
    let ids = bundle
        .authoritative_records()
        .into_iter()
        .map(BundleRecord::id)
        .collect::<Vec<_>>();
    if accounted.iter().map(String::as_str).collect::<Vec<_>>() != ids {
        return Err(ValidationFailure::Source);
    }
    reject_duplicates(accounted, ValidationFailure::Source)?;
    Ok(ids)
}
