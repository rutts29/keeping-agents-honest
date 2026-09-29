use super::*;

pub(super) fn conflict_raised(
    state: &mut ReplayState,
    conflict: &Conflict,
    approval: &Option<Approval>,
) -> Result<(), HistoryError> {
    ensure_started(state, "conflict_raised")?;
    validate_conflict(conflict, approval.as_ref(), "conflict_raised")?;
    if state.conflicts.contains_key(&conflict.id) {
        return Err(HistoryError::DuplicateId {
            kind: "conflict",
            id: conflict.id.to_string(),
        });
    }
    for intent_id in &conflict.intent_ids {
        let item = state
            .intents
            .get(intent_id)
            .ok_or_else(|| HistoryError::UnknownIntent {
                id: intent_id.clone(),
            })?;
        if !item.is_active() {
            return Err(HistoryError::InvalidTransition {
                event: "conflict_raised",
                reason: "a conflict may only affect active intents".to_owned(),
            });
        }
    }
    for intent_id in &conflict.intent_ids {
        let item = state
            .intents
            .get_mut(intent_id)
            .expect("validated intent id exists");
        item.lifecycle = IntentLifecycle::Conflicted;
        append_sources(&mut item.changed_by, &conflict.source_refs);
        if let Some(approval) = approval {
            append_sources(&mut item.changed_by, &approval.source_refs);
        }
    }
    state.conflicts.insert(
        conflict.id.clone(),
        ConflictState {
            conflict: conflict.clone(),
            resolved: false,
        },
    );
    Ok(())
}

pub(super) fn conflict_resolved(
    state: &mut ReplayState,
    conflict_id: &ConflictId,
    resolution: &ConflictResolution,
) -> Result<(), HistoryError> {
    ensure_started(state, "conflict_resolved")?;
    validate_authority(
        &resolution.source_refs,
        resolution.approval.as_ref(),
        "conflict_resolved",
    )?;
    non_empty(&resolution.alternative_id, "conflict alternative id")?;
    let conflict =
        state
            .conflicts
            .get(conflict_id)
            .ok_or_else(|| HistoryError::UnknownConflict {
                id: conflict_id.clone(),
            })?;
    if conflict.resolved {
        return Err(HistoryError::InvalidTransition {
            event: "conflict_resolved",
            reason: "conflict is already resolved".to_owned(),
        });
    }
    let selected_text = conflict
        .conflict
        .alternatives
        .iter()
        .find(|alternative| alternative.id == resolution.alternative_id)
        .map(|alternative| alternative.text.clone())
        .ok_or_else(|| HistoryError::InvalidTransition {
            event: "conflict_resolved",
            reason: "resolution does not select a live alternative".to_owned(),
        })?;
    let intent_ids = conflict.conflict.intent_ids.clone();
    validate_conflicted_intents(state, &intent_ids)?;
    for intent_id in intent_ids {
        let item = state
            .intents
            .get_mut(&intent_id)
            .expect("validated intent id exists");
        item.lifecycle = IntentLifecycle::Active;
        item.text.clone_from(&selected_text);
        append_sources(&mut item.changed_by, &resolution.source_refs);
        if let Some(approval) = &resolution.approval {
            append_sources(&mut item.changed_by, &approval.source_refs);
        }
    }
    state
        .conflicts
        .get_mut(conflict_id)
        .expect("validated conflict id exists")
        .resolved = true;
    Ok(())
}

fn validate_conflicted_intents(
    state: &ReplayState,
    intent_ids: &[IntentId],
) -> Result<(), HistoryError> {
    for intent_id in intent_ids {
        let item = state
            .intents
            .get(intent_id)
            .ok_or_else(|| HistoryError::UnknownIntent {
                id: intent_id.clone(),
            })?;
        if !matches!(item.lifecycle, IntentLifecycle::Conflicted) {
            return Err(HistoryError::InvalidTransition {
                event: "conflict_resolved",
                reason: "affected intent is no longer conflicted".to_owned(),
            });
        }
    }
    Ok(())
}
