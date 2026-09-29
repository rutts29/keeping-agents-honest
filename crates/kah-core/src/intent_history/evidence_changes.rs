use super::*;

pub(super) fn evidence_attached(
    state: &mut ReplayState,
    intent_id: &IntentId,
    evidence: &EvidenceRef,
) -> Result<(), HistoryError> {
    ensure_started(state, "evidence_attached")?;
    validate_evidence(evidence, "evidence_attached")?;
    if state
        .intents
        .values()
        .any(|item| item.evidence.iter().any(|prior| prior.id == evidence.id))
    {
        return Err(HistoryError::DuplicateId {
            kind: "evidence",
            id: evidence.id.to_string(),
        });
    }
    let item = state
        .intents
        .get_mut(intent_id)
        .ok_or_else(|| HistoryError::UnknownIntent {
            id: intent_id.clone(),
        })?;
    item.evidence.push(evidence.clone());
    item.evidence_state = EvidenceState::Satisfied;
    Ok(())
}

pub(super) fn evidence_invalidated(
    state: &mut ReplayState,
    intent_id: &IntentId,
    evidence_id: &EvidenceId,
    source_refs: &[SourceRef],
    reason: &str,
) -> Result<(), HistoryError> {
    ensure_started(state, "evidence_invalidated")?;
    validate_source_refs(source_refs, "evidence_invalidated")?;
    non_empty(reason, "evidence invalidation reason")?;
    let item = state
        .intents
        .get(intent_id)
        .ok_or_else(|| HistoryError::UnknownIntent {
            id: intent_id.clone(),
        })?;
    if !item
        .evidence
        .iter()
        .any(|evidence| evidence.id == *evidence_id)
    {
        return Err(HistoryError::UnknownEvidence {
            id: evidence_id.clone(),
        });
    }
    if !state.evidence_invalidated.insert(evidence_id.clone()) {
        return Err(HistoryError::InvalidTransition {
            event: "evidence_invalidated",
            reason: "evidence is already invalidated".to_owned(),
        });
    }
    let item = state
        .intents
        .get_mut(intent_id)
        .expect("validated intent id exists");
    item.evidence_state = if item
        .evidence
        .iter()
        .any(|evidence| !state.evidence_invalidated.contains(&evidence.id))
    {
        EvidenceState::Satisfied
    } else {
        EvidenceState::Reopened
    };
    Ok(())
}
