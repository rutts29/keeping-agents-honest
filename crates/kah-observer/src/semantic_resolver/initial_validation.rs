use super::*;

pub(super) fn validate_and_project(
    bundle: &NeutralSessionBundle,
    proposal: ProjectionProposal,
    projection_config: ProjectionConfig,
) -> Result<(History, ActiveProjection), ValidationFailure> {
    validate_proposal(bundle, proposal).and_then(|history| {
        ActiveProjection::from_history(&history, projection_config)
            .map(|projection| (history, projection))
            .map_err(|_| ValidationFailure::Projection)
    })
}

pub(super) fn validate_proposal(
    bundle: &NeutralSessionBundle,
    proposal: ProjectionProposal,
) -> Result<History, ValidationFailure> {
    bundle
        .validate_for_projection()
        .map_err(|_| ValidationFailure::Source)?;
    if proposal.schema_version != PROPOSAL_SCHEMA_VERSION {
        return Err(ValidationFailure::SchemaVersion);
    }
    if proposal.goal.text.trim().is_empty() {
        return Err(ValidationFailure::EmptyGoal);
    }
    if proposal.operations.is_empty() {
        return Err(ValidationFailure::EmptyOperations);
    }

    let source_refs = bundle.source_refs();
    let source_lookup: BTreeMap<&str, (&BundleRecord, &SourceRef)> = bundle
        .records()
        .iter()
        .zip(source_refs.iter())
        .map(|(record, source)| (record.id(), (record, source)))
        .collect();
    let authoritative_ids = accounted_user_ids(bundle, &proposal.accounted_source_record_ids)?;

    let mut referenced_sources = BTreeSet::new();
    let goal_sources = resolve_sources(
        &proposal.goal.source_record_ids,
        &source_lookup,
        &mut referenced_sources,
    )?;
    let history = History::new(GoalRevision::new(
        1,
        proposal.goal.text.trim(),
        goal_sources,
    ))
    .map_err(|_| ValidationFailure::Transition)?;

    let mut operations = OperationState {
        history,
        lookup: source_lookup,
        digest: bundle.source().digest(),
        referenced: referenced_sources,
        keys: BTreeSet::new(),
        intents: BTreeMap::new(),
    };
    for operation in proposal.operations {
        operations.initial(operation)?;
    }
    let history = operations.history;
    let referenced_sources = operations.referenced;

    let referenced: Vec<&str> = authoritative_ids
        .iter()
        .copied()
        .filter(|id| referenced_sources.contains(*id))
        .collect();
    if referenced != authoritative_ids {
        return Err(ValidationFailure::Source);
    }
    Ok(history)
}
