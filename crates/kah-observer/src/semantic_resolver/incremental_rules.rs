use super::*;

pub(super) fn validate_incremental_base(
    history: &History,
    projection: &ActiveProjection,
    delta: &NeutralSessionBundle,
) -> Result<(), ValidationFailure> {
    delta
        .validate_for_projection()
        .map_err(|_| ValidationFailure::Source)?;
    let sequence = history
        .records()
        .last()
        .map(|record| record.sequence)
        .ok_or(ValidationFailure::StaleBase)?;
    if projection.revision != sequence || projection.source_head.pending_sequence != sequence {
        return Err(ValidationFailure::StaleBase);
    }
    if projection.continuation_blocked() {
        return Err(ValidationFailure::Projection);
    }
    let Some(history_source) = history.goal().source_refs.first() else {
        return Err(ValidationFailure::Source);
    };
    if delta.source().provider() != history_source.provider.clone()
        || delta.source().session_ref_private() != history_source.session_private()
    {
        return Err(ValidationFailure::Source);
    }
    let expected =
        ActiveProjection::from_history(history, ProjectionConfig::new(projection.overflow.budget))
            .map_err(|_| ValidationFailure::Projection)?;
    if expected.rendered_prompt() != projection.rendered_prompt()
        || expected.source_head != projection.source_head
        || expected.overflow != projection.overflow
        || expected.closure != projection.closure
    {
        return Err(ValidationFailure::StaleBase);
    }
    Ok(())
}

pub(super) fn validate_incremental_classification(
    classification: IncrementalClassification,
    operations: &[IncrementalOperationProposal],
    proposed_goal: Option<&IncrementalGoalProposal>,
) -> Result<(), ValidationFailure> {
    let valid = match classification {
        IncrementalClassification::NoChange => proposed_goal.is_none() && operations.is_empty(),
        IncrementalClassification::Additive => {
            proposed_goal.is_none()
                && !operations.is_empty()
                && operations
                    .iter()
                    .all(|operation| operation.operation == IncrementalOperationName::Add)
        }
        IncrementalClassification::Supersession => {
            proposed_goal.is_none()
                && !operations.is_empty()
                && operations.iter().all(|operation| {
                    matches!(
                        operation.operation,
                        IncrementalOperationName::Add | IncrementalOperationName::Supersede
                    )
                })
                && operations
                    .iter()
                    .any(|operation| operation.operation == IncrementalOperationName::Supersede)
        }
        IncrementalClassification::Withdrawal => {
            proposed_goal.is_none()
                && !operations.is_empty()
                && operations.iter().all(|operation| {
                    matches!(
                        operation.operation,
                        IncrementalOperationName::Add | IncrementalOperationName::Withdraw
                    )
                })
                && operations
                    .iter()
                    .any(|operation| operation.operation == IncrementalOperationName::Withdraw)
        }
        IncrementalClassification::Conflict => {
            proposed_goal.is_none()
                && !operations.is_empty()
                && operations.iter().all(|operation| {
                    matches!(
                        operation.operation,
                        IncrementalOperationName::Add | IncrementalOperationName::Conflict
                    )
                })
                && operations
                    .iter()
                    .any(|operation| operation.operation == IncrementalOperationName::Conflict)
        }
        IncrementalClassification::Reopen => {
            proposed_goal.is_none()
                && !operations.is_empty()
                && operations
                    .iter()
                    .all(|operation| operation.operation == IncrementalOperationName::Reopen)
        }
        IncrementalClassification::GoalChange => proposed_goal.is_some() && operations.is_empty(),
    };
    valid.then_some(()).ok_or(ValidationFailure::Transition)
}

pub(super) fn validate_operation_targets(
    operations: &[IncrementalOperationProposal],
    active_ids: &BTreeSet<String>,
) -> Result<(), ValidationFailure> {
    for operation in operations {
        let targets_are_active = match operation.operation {
            IncrementalOperationName::Add => true,
            IncrementalOperationName::Supersede
            | IncrementalOperationName::Withdraw
            | IncrementalOperationName::Reopen => active_ids.contains(&operation.target_intent_id),
            IncrementalOperationName::Conflict => operation
                .intent_ids
                .iter()
                .all(|intent_id| active_ids.contains(intent_id)),
        };
        if !targets_are_active {
            return Err(ValidationFailure::Transition);
        }
    }
    Ok(())
}

pub(super) fn validate_incremental_new_clause(
    operation: &IncrementalOperationProposal,
    supersession: bool,
) -> Result<(), ValidationFailure> {
    validate_key(&operation.key)?;
    if operation.text.trim().is_empty() {
        return Err(ValidationFailure::EmptyClause);
    }
    if (supersession && operation.target_intent_id.is_empty())
        || (!supersession && !operation.target_intent_id.is_empty())
        || !operation.intent_ids.is_empty()
        || !operation.evidence_id.is_empty()
        || !operation.reason.is_empty()
        || !operation.alternatives.is_empty()
    {
        return Err(ValidationFailure::AddIrrelevantFields);
    }
    Ok(())
}

pub(super) fn validate_incremental_terminal_shape(
    operation: &IncrementalOperationProposal,
    reopen: bool,
) -> Result<(), ValidationFailure> {
    if !operation.key.is_empty()
        || !operation.text.is_empty()
        || operation.target_intent_id.is_empty()
        || !operation.intent_ids.is_empty()
        || (!reopen && (!operation.evidence_id.is_empty() || !operation.reason.is_empty()))
        || (reopen && (operation.evidence_id.is_empty() || operation.reason.trim().is_empty()))
        || !operation.alternatives.is_empty()
    {
        return Err(ValidationFailure::Transition);
    }
    Ok(())
}

pub(super) fn validate_incremental_conflict_shape(
    operation: &IncrementalOperationProposal,
) -> Result<(), ValidationFailure> {
    validate_key(&operation.key)?;
    reject_duplicates(&operation.intent_ids, ValidationFailure::ConflictShape)?;
    if operation.text.trim().is_empty()
        || !operation.target_intent_id.is_empty()
        || operation.intent_ids.is_empty()
        || !operation.evidence_id.is_empty()
        || !operation.reason.is_empty()
        || operation.alternatives.len() < 2
    {
        return Err(ValidationFailure::ConflictShape);
    }
    Ok(())
}
