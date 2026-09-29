use super::*;

pub(super) fn ensure_started(state: &ReplayState, event: &'static str) -> Result<(), HistoryError> {
    if state.goal.is_none() {
        Err(if event == "run_started" {
            HistoryError::RunStartedRequired
        } else {
            HistoryError::InvalidTransition {
                event,
                reason: "event requires a preceding run_started".to_owned(),
            }
        })
    } else {
        Ok(())
    }
}

pub(super) fn validate_new_item(
    item: &IntentItem,
    approval: Option<&Approval>,
    event: &'static str,
) -> Result<(), HistoryError> {
    validate_intent_id(&item.id, "intent id")?;
    non_empty(&item.text, "intent text")?;
    validate_source_refs(&item.introduced_by, event)?;
    validate_authority(&item.introduced_by, approval, event)?;
    validate_source_refs(&item.changed_by, event).or_else(|error| {
        if item.changed_by.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    })?;
    if !matches!(item.lifecycle, IntentLifecycle::Active)
        || !matches!(item.evidence_state, EvidenceState::Unresolved)
    {
        return Err(HistoryError::InvalidTransition {
            event,
            reason: "new intent must be active with unresolved evidence".to_owned(),
        });
    }
    if !item.evidence.is_empty() {
        return Err(HistoryError::InvalidTransition {
            event,
            reason: "new intent cannot contain pre-attached evidence".to_owned(),
        });
    }
    for superseded_id in &item.supersedes {
        validate_intent_id(superseded_id, "superseded intent id")?;
    }
    Ok(())
}

pub(super) fn validate_intent_id(id: &IntentId, field: &'static str) -> Result<(), HistoryError> {
    non_empty(id.as_str(), field)
}

pub(super) fn validate_goal(
    goal: &GoalRevision,
    initial: bool,
    event: &'static str,
) -> Result<(), HistoryError> {
    non_empty(&goal.text, "goal text")?;
    validate_authority(&goal.source_refs, goal.approval.as_ref(), event)?;
    if initial && goal.revision == 0 {
        return Err(HistoryError::InvalidTransition {
            event,
            reason: "goal revision must be positive".to_owned(),
        });
    }
    Ok(())
}

pub(super) fn validate_conflict(
    conflict: &Conflict,
    approval: Option<&Approval>,
    event: &'static str,
) -> Result<(), HistoryError> {
    validate_intent_id(&IntentId::new(conflict.id.as_str()), "conflict id")?;
    validate_source_refs(&conflict.source_refs, event)?;
    validate_authority(&conflict.source_refs, approval, event)?;
    if conflict.intent_ids.is_empty() {
        return Err(HistoryError::InvalidTransition {
            event,
            reason: "conflict must affect at least one intent".to_owned(),
        });
    }
    let mut intent_ids = BTreeSet::new();
    for intent_id in &conflict.intent_ids {
        validate_intent_id(intent_id, "conflict intent id")?;
        if !intent_ids.insert(intent_id) {
            return Err(HistoryError::InvalidTransition {
                event,
                reason: "conflict intent ids must be unique".to_owned(),
            });
        }
    }
    if conflict.alternatives.len() < 2 {
        return Err(HistoryError::InvalidTransition {
            event,
            reason: "conflict requires at least two alternatives".to_owned(),
        });
    }
    let mut alternative_ids = BTreeSet::new();
    for alternative in &conflict.alternatives {
        non_empty(&alternative.id, "conflict alternative id")?;
        non_empty(&alternative.text, "conflict alternative text")?;
        validate_source_refs(&alternative.source_refs, event)?;
        if !alternative_ids.insert(&alternative.id) {
            return Err(HistoryError::InvalidTransition {
                event,
                reason: "conflict alternative ids must be unique".to_owned(),
            });
        }
    }
    Ok(())
}

pub(super) fn validate_evidence(
    evidence: &EvidenceRef,
    event: &'static str,
) -> Result<(), HistoryError> {
    non_empty(evidence.id.as_str(), "evidence id")?;
    non_empty(&evidence.summary, "evidence summary")?;
    validate_source_refs(&evidence.source_refs, event)
}

pub(super) fn validate_authority(
    source_refs: &[SourceRef],
    approval: Option<&Approval>,
    event: &'static str,
) -> Result<(), HistoryError> {
    validate_source_refs(source_refs, event)?;
    if source_refs
        .iter()
        .any(|source| matches!(source.role, SourceRole::User))
    {
        if let Some(approval) = approval {
            approval.validate(event)?;
        }
        return Ok(());
    }
    let Some(approval) = approval else {
        return Err(HistoryError::UnauthorizedSource { event });
    };
    approval.validate(event)
}

pub(super) fn validate_source_refs(
    source_refs: &[SourceRef],
    event: &'static str,
) -> Result<(), HistoryError> {
    if source_refs.is_empty() {
        return Err(HistoryError::MissingSourceRefs { event });
    }
    for source in source_refs {
        source.validate()?;
    }
    Ok(())
}

pub(super) fn non_empty(value: &str, field: &'static str) -> Result<(), HistoryError> {
    if value.trim().is_empty() {
        Err(HistoryError::InvalidSourceRef { field })
    } else {
        Ok(())
    }
}

pub(super) fn append_sources(target: &mut Vec<SourceRef>, sources: &[SourceRef]) {
    for source in sources {
        if !target.contains(source) {
            target.push(source.clone());
        }
    }
}
