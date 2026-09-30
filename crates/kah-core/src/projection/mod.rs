//! Deterministic, bounded active-intent projections.
//!
//! This module folds the already validated [`crate::intent_history::History`]
//! state into the prompt-facing projection.  It deliberately does not call a
//! model or mutate history: the source history remains the audit record, and
//! this module only derives a new value from it.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::intent_history::{
    Conflict, Event, EvidenceState, GoalRevision, History, HistoryError, IntentItem, IntentKind,
    IntentLifecycle, SourceRef,
};

mod models;
mod validation;
pub use models::*;
use validation::*;

/// Fold a validated history into a deterministic active projection.
pub fn project<C>(history: &History, config: C) -> Result<ActiveProjection, ProjectionError>
where
    C: Into<ProjectionConfig>,
{
    let config = config.into();
    let snapshot = history.snapshot();
    validate_goal(&snapshot.goal)?;

    for item in snapshot.intents.values() {
        validate_item(item)?;
    }
    validate_supersession_edges(&snapshot.intents)?;

    let mut preserve = Vec::new();
    let mut frontier = Vec::new();
    let mut validation = Vec::new();
    for item in snapshot.intents.values().filter(|item| item.is_active()) {
        match item.kind {
            IntentKind::Validation | IntentKind::StopCondition => validation.push(item.clone()),
            _ if matches!(item.evidence_state, EvidenceState::Satisfied) => {
                preserve.push(item.clone());
            }
            _ => frontier.push(item.clone()),
        }
    }

    let mut conflicts = Vec::new();
    for conflict in snapshot.open_conflicts.values() {
        validate_conflict(conflict, &snapshot.intents)?;
        conflicts.push(stabilize_conflict(conflict));
    }

    let blockers = closure_blockers(&frontier, &validation, &conflicts);

    let item_count = preserve.len()
        + frontier.len()
        + validation.len()
        + conflicts
            .iter()
            .map(|conflict| conflict.intent_ids.len())
            .sum::<usize>();
    let source_head = source_head(history)?;
    let mut projection = ActiveProjection {
        compaction: None,
        schema_version: PROJECTION_SCHEMA_VERSION,
        revision: source_head.pending_sequence,
        source_head,
        goal: snapshot.goal,
        preserve,
        frontier,
        validation,
        conflicts,
        closure: ClosureState { blockers },
        overflow: ProjectionOverflow {
            bytes: 0,
            budget: config.byte_budget,
            item_count,
            reason: None,
        },
        generated_by: ProjectionGeneratedBy {
            component: "driftctl.projection".to_owned(),
            model: None,
            reasoning: None,
            prompt_schema_version: PROJECTION_SCHEMA_VERSION,
        },
    };

    account_overflow(&mut projection)?;

    Ok(projection)
}

fn closure_blockers(
    frontier: &[IntentItem],
    validation: &[IntentItem],
    conflicts: &[Conflict],
) -> Vec<ClosureBlocker> {
    let mut blockers = Vec::new();
    for item in frontier {
        blockers.push(ClosureBlocker {
            kind: ClosureBlockerKind::UnresolvedRequirement,
            id: Some(item.id.to_string()),
            reason: "active intent has unresolved or reopened evidence".to_owned(),
            source_refs: item.introduced_by.clone(),
        });
    }
    for item in validation {
        if !matches!(item.evidence_state, EvidenceState::Satisfied) {
            blockers.push(ClosureBlocker {
                kind: ClosureBlockerKind::UnresolvedRequirement,
                id: Some(item.id.to_string()),
                reason: "validation obligation has unresolved or reopened evidence".to_owned(),
                source_refs: item.introduced_by.clone(),
            });
        }
    }
    for conflict in conflicts {
        blockers.push(ClosureBlocker {
            kind: ClosureBlockerKind::Conflict,
            id: Some(conflict.id.to_string()),
            reason: "unresolved semantic conflict blocks continuation".to_owned(),
            source_refs: conflict.source_refs.clone(),
        });
    }

    blockers
}

fn account_overflow(projection: &mut ActiveProjection) -> Result<(), ProjectionError> {
    let bytes = projection.try_rendered_prompt()?.len();
    projection.overflow.bytes = bytes;
    if bytes > projection.overflow.budget {
        projection.overflow.reason = Some(format!(
            "rendered prompt payload is {bytes} bytes, over budget {}",
            projection.overflow.budget
        ));
        projection.closure.blockers.push(ClosureBlocker {
            kind: ClosureBlockerKind::ProjectionOverflow,
            id: None,
            reason: projection
                .overflow
                .reason
                .clone()
                .expect("overflow reason was set"),
            source_refs: Vec::new(),
        });
    }
    Ok(())
}

fn source_head(history: &History) -> Result<ProjectionSourceHead, ProjectionError> {
    let source_sequence = history
        .records()
        .last()
        .map(|record| record.sequence)
        .ok_or_else(|| ProjectionError::InvalidState {
            reason: "history has no source records".to_owned(),
        })?;
    Ok(ProjectionSourceHead {
        record: latest_source_record(history).ok_or_else(|| {
            ProjectionError::MissingAccountability {
                context: "history source head".to_owned(),
            }
        })?,
        pending_sequence: source_sequence,
    })
}
