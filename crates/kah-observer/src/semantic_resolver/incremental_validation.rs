use super::*;

pub(super) fn validate_incremental_and_project(
    history: &History,
    projection: &ActiveProjection,
    delta: &NeutralSessionBundle,
    proposal: IncrementalProjectionProposal,
    projection_config: ProjectionConfig,
) -> Result<(History, ActiveProjection, Option<GoalChangeObservation>), ValidationFailure> {
    let expected_active_ids = validate_header(history, projection, delta, &proposal)?;

    let source_refs = delta.source_refs();
    let source_lookup: BTreeMap<&str, (&BundleRecord, &SourceRef)> = delta
        .records()
        .iter()
        .zip(source_refs.iter())
        .map(|(record, source)| (record.id(), (record, source)))
        .collect();
    let authoritative_ids = accounted_user_ids(delta, &proposal.accounted_source_record_ids)?;
    let prior_source_record_ids = history_source_record_ids(history);
    if delta
        .records()
        .iter()
        .any(|record| prior_source_record_ids.contains(record.id()))
    {
        return Err(ValidationFailure::Source);
    }

    validate_incremental_classification(
        proposal.classification,
        &proposal.operations,
        proposal.proposed_goal.as_ref(),
    )?;
    validate_operation_targets(&proposal.operations, &expected_active_ids)?;
    let next = history.clone();
    let mut referenced_sources = BTreeSet::new();
    if proposal.classification == IncrementalClassification::NoChange {
        referenced_sources.extend(authoritative_ids.iter().map(|id| (*id).to_owned()));
    }
    let goal_change =
        observe_goal_change(history, &proposal, &source_lookup, &mut referenced_sources)?;
    let mut operations = OperationState {
        history: next,
        lookup: source_lookup,
        digest: delta.source().digest(),
        referenced: referenced_sources,
        keys: BTreeSet::new(),
        intents: BTreeMap::new(),
    };
    for operation in proposal.operations {
        operations.incremental(operation)?;
    }
    let next = operations.history;
    let referenced_sources = operations.referenced;

    if !authoritative_ids
        .iter()
        .all(|id| referenced_sources.contains(*id))
    {
        return Err(ValidationFailure::Source);
    }
    let next_projection = ActiveProjection::from_history(&next, projection_config)
        .map_err(|_| ValidationFailure::Projection)?;
    if next_projection.is_overflowed() {
        return Err(ValidationFailure::Projection);
    }
    Ok((next, next_projection, goal_change))
}

fn validate_header(
    history: &History,
    projection: &ActiveProjection,
    delta: &NeutralSessionBundle,
    proposal: &IncrementalProjectionProposal,
) -> Result<BTreeSet<String>, ValidationFailure> {
    validate_incremental_base(history, projection, delta)?;
    if proposal.schema_version != PROPOSAL_SCHEMA_VERSION {
        return Err(ValidationFailure::SchemaVersion);
    }
    let event_sequence = history
        .records()
        .last()
        .map(|record| record.sequence)
        .ok_or(ValidationFailure::StaleBase)?;
    if proposal.base_projection_revision != projection.revision
        || proposal.base_event_sequence != event_sequence
    {
        return Err(ValidationFailure::StaleBase);
    }
    reject_duplicates(
        &proposal.accounted_active_intent_ids,
        ValidationFailure::ActiveAccounting,
    )?;
    let expected_active_ids = projection_intent_ids(projection)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if proposal
        .accounted_active_intent_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        != expected_active_ids
    {
        return Err(ValidationFailure::ActiveAccounting);
    }

    Ok(expected_active_ids)
}

fn observe_goal_change(
    history: &History,
    proposal: &IncrementalProjectionProposal,
    source_lookup: &BTreeMap<&str, (&BundleRecord, &SourceRef)>,
    referenced_sources: &mut BTreeSet<String>,
) -> Result<Option<GoalChangeObservation>, ValidationFailure> {
    let result = if let Some(proposed_goal) = &proposal.proposed_goal {
        if proposed_goal.text.trim().is_empty() || proposed_goal.text.trim() == history.goal().text
        {
            return Err(ValidationFailure::GoalChange);
        }
        let source_refs = resolve_sources(
            &proposed_goal.source_record_ids,
            source_lookup,
            referenced_sources,
        )?;
        Some(GoalChangeObservation {
            proposed_goal: proposed_goal.text.trim().to_owned(),
            source_refs,
            base_projection_revision: proposal.base_projection_revision,
            base_event_sequence: proposal.base_event_sequence,
        })
    } else {
        None
    };
    Ok(result)
}
