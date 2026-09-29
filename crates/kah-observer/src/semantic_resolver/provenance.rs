use super::*;

pub(super) fn projection_intent_ids(projection: &ActiveProjection) -> Vec<String> {
    let mut ids = projection
        .preserve
        .iter()
        .chain(&projection.frontier)
        .chain(&projection.validation)
        .map(|item| item.id.to_string())
        .chain(
            projection
                .conflicts
                .iter()
                .flat_map(|conflict| conflict.intent_ids.iter().map(ToString::to_string)),
        )
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

pub(super) fn history_source_record_ids(history: &History) -> BTreeSet<String> {
    history
        .records()
        .iter()
        .flat_map(|record| event_source_refs(&record.event))
        .map(|source| source.record.clone())
        .collect()
}

pub(super) fn event_source_refs(event: &Event) -> Vec<&SourceRef> {
    match event {
        Event::RunStarted { goal } | Event::GoalRevised { goal } => goal_sources(goal),
        Event::RequirementAdded { item, approval }
        | Event::RequirementSuperseded {
            replacement: item,
            approval,
            ..
        } => item
            .introduced_by
            .iter()
            .chain(&item.changed_by)
            .chain(
                item.evidence
                    .iter()
                    .flat_map(|evidence| &evidence.source_refs),
            )
            .chain(approval.iter().flat_map(|approval| &approval.source_refs))
            .collect(),
        Event::RequirementWithdrawn {
            source_refs,
            approval,
            ..
        }
        | Event::RunClosed {
            source_refs,
            approval,
        } => source_refs
            .iter()
            .chain(approval.iter().flat_map(|approval| &approval.source_refs))
            .collect(),
        Event::ConflictRaised { conflict, approval } => conflict
            .source_refs
            .iter()
            .chain(
                conflict
                    .alternatives
                    .iter()
                    .flat_map(|alternative| &alternative.source_refs),
            )
            .chain(approval.iter().flat_map(|approval| &approval.source_refs))
            .collect(),
        Event::ConflictResolved { resolution, .. } => resolution
            .source_refs
            .iter()
            .chain(
                resolution
                    .approval
                    .iter()
                    .flat_map(|approval| &approval.source_refs),
            )
            .collect(),
        Event::EvidenceAttached { evidence, .. } => evidence.source_refs.iter().collect(),
        Event::EvidenceInvalidated { source_refs, .. } => source_refs.iter().collect(),
    }
}

fn goal_sources(goal: &GoalRevision) -> Vec<&SourceRef> {
    goal.source_refs
        .iter()
        .chain(
            goal.approval
                .iter()
                .flat_map(|approval| &approval.source_refs),
        )
        .collect()
}
