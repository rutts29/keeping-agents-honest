//! Durable inspection extracted from driftctl cli.rs.
use crate::goal_change_store::GoalChangeStore;
use crate::semantic_resolver::{
    self, InspectResolution, NativeGoalObservation, ResolverMetadata, ResolverUsage,
};
use kah_core::run_store::RunStore;
use kah_core::session_bundle::NativeGoal;

pub fn cached_resolution(
    recovered: kah_core::run_store::RecoveredRun,
    native_goal: Option<&NativeGoal>,
    goal_change: Option<semantic_resolver::GoalChangeObservation>,
) -> InspectResolution {
    let generated = &recovered.projection.generated_by;
    let metadata = ResolverMetadata {
        model: generated
            .model
            .clone()
            .unwrap_or_else(|| "stored-projection".to_owned()),
        reasoning: generated
            .reasoning
            .clone()
            .unwrap_or_else(|| "unknown".to_owned()),
        calls: 0,
        elapsed_ms: 0,
        usage: ResolverUsage::default(),
        usage_complete: true,
        prompt_schema_version: generated.prompt_schema_version,
        proposal_schema_version: 1,
        last_validation_failure: None,
        artifact_ids: Vec::new(),
    };
    let text_private = native_goal.and_then(NativeGoal::text).map(str::to_owned);
    let native_goal = NativeGoalObservation {
        state: native_goal.map_or_else(|| "unknown".to_owned(), |goal| goal.state().to_owned()),
        conflicts_with_projection: text_private
            .as_deref()
            .is_some_and(|text| text != recovered.projection.goal.text),
        text_private,
    };
    InspectResolution {
        history: recovered.history,
        projection: recovered.projection,
        metadata,
        native_goal,
        goal_change,
    }
}

pub fn accepted_event_sequence(
    recovered: &kah_core::run_store::RecoveredRun,
) -> Result<u64, String> {
    recovered
        .history
        .records()
        .last()
        .map(|record| record.sequence)
        .ok_or_else(|| {
            "INSPECT_SEQUENCE_MISSING stage=inspection: stored run has no accepted event sequence"
                .to_owned()
        })
}

pub fn load_goal_change(
    run: &RunStore,
    recovered: &kah_core::run_store::RecoveredRun,
    source_digest: &str,
) -> Result<Option<semantic_resolver::GoalChangeObservation>, String> {
    let store = GoalChangeStore::open(run).map_err(|error| error.to_string())?;
    let Some(proposal) = store.load_pending().map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let event_sequence = accepted_event_sequence(recovered)?;
    if !proposal.matches(recovered.projection.revision, event_sequence, source_digest) {
        return Err("INSPECT_GOAL_STALE stage=inspection: pending goal change is stale for the accepted source".to_owned());
    }
    Ok(Some(proposal.observation()))
}
