use super::*;

pub(super) fn validate_goal(goal: &GoalRevision) -> Result<(), ProjectionError> {
    if goal.text.trim().is_empty() {
        return Err(ProjectionError::MissingAccountability {
            context: "goal text".to_owned(),
        });
    }
    require_sources(&goal.source_refs, "goal")?;
    if let Some(approval) = &goal.approval {
        require_sources(&approval.source_refs, "goal approval")?;
    }
    Ok(())
}

pub(super) fn validate_item(item: &IntentItem) -> Result<(), ProjectionError> {
    require_sources(&item.introduced_by, &format!("intent {}", item.id))?;
    if !item.is_active() && item.changed_by.is_empty() {
        return Err(ProjectionError::MissingAccountability {
            context: format!("lifecycle transition for intent {}", item.id),
        });
    }
    if !item.supersedes.is_empty() && item.changed_by.is_empty() {
        return Err(ProjectionError::MissingAccountability {
            context: format!("supersession for intent {}", item.id),
        });
    }
    validate_sources(&item.changed_by, &format!("intent {} changes", item.id))?;
    for evidence in &item.evidence {
        if evidence.summary.trim().is_empty() {
            return Err(ProjectionError::MissingAccountability {
                context: format!("evidence {} summary", evidence.id),
            });
        }
        require_sources(
            &evidence.source_refs,
            &format!("evidence {} for intent {}", evidence.id, item.id),
        )?;
    }
    if matches!(item.evidence_state, EvidenceState::Satisfied) && item.evidence.is_empty() {
        return Err(ProjectionError::InvalidState {
            reason: format!("satisfied intent {} has no evidence", item.id),
        });
    }
    Ok(())
}

pub(super) fn validate_supersession_edges(
    intents: &std::collections::BTreeMap<crate::intent_history::IntentId, IntentItem>,
) -> Result<(), ProjectionError> {
    for item in intents.values() {
        for previous_id in &item.supersedes {
            let Some(previous) = intents.get(previous_id) else {
                return Err(ProjectionError::InvalidState {
                    reason: format!(
                        "intent {} supersedes missing intent {}",
                        item.id, previous_id
                    ),
                });
            };
            if previous.is_active() {
                return Err(ProjectionError::InvalidState {
                    reason: format!(
                        "intent {} supersedes active intent {}",
                        item.id, previous_id
                    ),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn validate_conflict(
    conflict: &Conflict,
    intents: &std::collections::BTreeMap<crate::intent_history::IntentId, IntentItem>,
) -> Result<(), ProjectionError> {
    require_sources(&conflict.source_refs, &format!("conflict {}", conflict.id))?;
    for alternative in &conflict.alternatives {
        require_sources(
            &alternative.source_refs,
            &format!("alternative {} in conflict {}", alternative.id, conflict.id),
        )?;
    }
    for intent_id in &conflict.intent_ids {
        let Some(item) = intents.get(intent_id) else {
            return Err(ProjectionError::InvalidState {
                reason: format!(
                    "conflict {} references missing intent {}",
                    conflict.id, intent_id
                ),
            });
        };
        if !matches!(item.lifecycle, IntentLifecycle::Conflicted) {
            return Err(ProjectionError::InvalidState {
                reason: format!(
                    "open conflict {} references non-conflicted intent {}",
                    conflict.id, intent_id
                ),
            });
        }
    }
    Ok(())
}

pub(super) fn stabilize_conflict(conflict: &Conflict) -> Conflict {
    let mut conflict = conflict.clone();
    conflict.intent_ids.sort();
    conflict
        .alternatives
        .sort_by(|left, right| left.id.cmp(&right.id));
    conflict
}

pub(super) fn require_sources(sources: &[SourceRef], context: &str) -> Result<(), ProjectionError> {
    if sources.is_empty() {
        return Err(ProjectionError::MissingAccountability {
            context: context.to_owned(),
        });
    }
    validate_sources(sources, context)
}

pub(super) fn validate_sources(
    sources: &[SourceRef],
    context: &str,
) -> Result<(), ProjectionError> {
    for source in sources {
        source
            .validate()
            .map_err(|error| ProjectionError::InvalidSource {
                context: context.to_owned(),
                source: error,
            })?;
    }
    Ok(())
}

pub(super) fn latest_source_record(history: &History) -> Option<String> {
    history
        .records()
        .iter()
        .rev()
        .find_map(|record| event_source_record(&record.event).map(str::to_owned))
}

pub(super) fn event_source_record(event: &Event) -> Option<&str> {
    let source = match event {
        Event::RunStarted { goal } | Event::GoalRevised { goal } => goal.source_refs.first(),
        Event::RequirementAdded { item, .. }
        | Event::RequirementSuperseded {
            replacement: item, ..
        } => item.introduced_by.first(),
        Event::RequirementConfirmed { source_refs, .. }
        | Event::RequirementWithdrawn { source_refs, .. }
        | Event::EvidenceInvalidated { source_refs, .. }
        | Event::RunClosed { source_refs, .. } => source_refs.first(),
        Event::ConflictRaised { conflict, .. } => conflict.source_refs.first(),
        Event::ConflictResolved { resolution, .. } => resolution.source_refs.first(),
        Event::EvidenceAttached { evidence, .. } => evidence.source_refs.first(),
    }?;
    Some(source.record.as_str())
}
