use super::*;

pub(super) fn run_started(
    state: &mut ReplayState,
    goal: &GoalRevision,
) -> Result<(), HistoryError> {
    if state.goal.is_some() {
        return Err(HistoryError::RunStartedNotFirst);
    }
    validate_goal(goal, true, "run_started")?;
    if goal.revision != 1 || goal.supersedes_revision.is_some() {
        return Err(HistoryError::InvalidTransition {
            event: "run_started",
            reason: "initial goal must be revision 1 without a superseded revision".to_owned(),
        });
    }
    state.goal = Some(goal.clone());
    Ok(())
}

pub(super) fn requirement_added(
    state: &mut ReplayState,
    item: &IntentItem,
    approval: &Option<Approval>,
) -> Result<(), HistoryError> {
    ensure_started(state, "requirement_added")?;
    validate_new_item(item, approval.as_ref(), "requirement_added")?;
    if state.intents.contains_key(&item.id) {
        return Err(HistoryError::DuplicateId {
            kind: "intent",
            id: item.id.to_string(),
        });
    }
    state.intents.insert(item.id.clone(), item.clone());
    Ok(())
}

pub(super) fn requirement_superseded(
    state: &mut ReplayState,
    previous_id: &IntentId,
    replacement: &IntentItem,
    approval: &Option<Approval>,
) -> Result<(), HistoryError> {
    ensure_started(state, "requirement_superseded")?;
    validate_new_item(replacement, approval.as_ref(), "requirement_superseded")?;
    if replacement.supersedes.as_slice() != [previous_id.clone()] {
        return Err(HistoryError::InvalidTransition {
            event: "requirement_superseded",
            reason: "replacement must contain exactly the superseded intent id".to_owned(),
        });
    }
    if state.intents.contains_key(&replacement.id) {
        return Err(HistoryError::DuplicateId {
            kind: "intent",
            id: replacement.id.to_string(),
        });
    }
    let old = state
        .intents
        .get_mut(previous_id)
        .ok_or_else(|| HistoryError::UnknownIntent {
            id: previous_id.clone(),
        })?;
    if !old.is_active() {
        return Err(HistoryError::InvalidTransition {
            event: "requirement_superseded",
            reason: "only an active intent can be superseded".to_owned(),
        });
    }
    old.lifecycle = IntentLifecycle::Superseded;
    append_sources(&mut old.changed_by, &replacement.introduced_by);
    let mut replacement = replacement.clone();
    if replacement.changed_by.is_empty() {
        replacement.changed_by = replacement.introduced_by.clone();
    }
    state.intents.insert(replacement.id.clone(), replacement);
    Ok(())
}

pub(super) fn requirement_withdrawn(
    state: &mut ReplayState,
    intent_id: &IntentId,
    source_refs: &[SourceRef],
    approval: &Option<Approval>,
) -> Result<(), HistoryError> {
    ensure_started(state, "requirement_withdrawn")?;
    validate_authority(source_refs, approval.as_ref(), "requirement_withdrawn")?;
    let item = state
        .intents
        .get_mut(intent_id)
        .ok_or_else(|| HistoryError::UnknownIntent {
            id: intent_id.clone(),
        })?;
    if !item.is_active() {
        return Err(HistoryError::InvalidTransition {
            event: "requirement_withdrawn",
            reason: "only an active intent can be withdrawn".to_owned(),
        });
    }
    item.lifecycle = IntentLifecycle::Withdrawn;
    append_sources(&mut item.changed_by, source_refs);
    if let Some(approval) = approval {
        append_sources(&mut item.changed_by, &approval.source_refs);
    }
    Ok(())
}

pub(super) fn goal_revised(
    state: &mut ReplayState,
    goal: &GoalRevision,
) -> Result<(), HistoryError> {
    ensure_started(state, "goal_revised")?;
    validate_goal(goal, false, "goal_revised")?;
    let current = state.goal.as_ref().expect("started history has a goal");
    let expected_revision =
        current
            .revision
            .checked_add(1)
            .ok_or_else(|| HistoryError::InvalidTransition {
                event: "goal_revised",
                reason: "goal revision exhausted".to_owned(),
            })?;
    if goal.revision != expected_revision || goal.supersedes_revision != Some(current.revision) {
        return Err(HistoryError::InvalidTransition {
            event: "goal_revised",
            reason: "goal revision must monotonically supersede the current revision".to_owned(),
        });
    }
    if goal.text == current.text {
        return Err(HistoryError::InvalidTransition {
            event: "goal_revised",
            reason: "goal revision must change the goal text".to_owned(),
        });
    }
    state.goal = Some(goal.clone());
    Ok(())
}

pub(super) fn run_closed(
    state: &mut ReplayState,
    source_refs: &[SourceRef],
    approval: &Option<Approval>,
) -> Result<(), HistoryError> {
    ensure_started(state, "run_closed")?;
    validate_authority(source_refs, approval.as_ref(), "run_closed")?;
    if state.conflicts.values().any(|conflict| !conflict.resolved) {
        return Err(HistoryError::InvalidTransition {
            event: "run_closed",
            reason: "open conflicts block closure".to_owned(),
        });
    }
    if state
        .intents
        .values()
        .any(|item| item.is_active() && !matches!(item.evidence_state, EvidenceState::Satisfied))
    {
        return Err(HistoryError::InvalidTransition {
            event: "run_closed",
            reason: "every active intent requires satisfied evidence".to_owned(),
        });
    }
    state.closed = true;
    Ok(())
}
