//! Same-session reconciliation and semantic commit controller.

use crate::enrollment::Enrollment;
use crate::goal_change_store::{GoalChangeStore, PendingGoalChange};
use crate::hook_event_store::HookEventStore;
use crate::inspect_state::InspectSource;
use crate::semantic_resolver::{self, InspectResolution};
use kah_codex::source::{self as codex_source, SessionSelection};
use kah_core::intent_history::{SourceProvider, SourceRole};
use kah_core::projection::{ActiveProjection, ProjectionConfig};
use kah_core::run_store::{RecoveredRun, RunStore, SourceCursor, SourceCursorComparison};
use kah_core::session_bundle::{BundleRecord, NativeGoal, NeutralSessionBundle};

pub enum KeeperDecision {
    Ready(ActiveProjection),
    Blocked {
        projection: ActiveProjection,
        reason: String,
    },
}

impl KeeperDecision {
    pub fn projection(&self) -> &ActiveProjection {
        match self {
            Self::Ready(projection) | Self::Blocked { projection, .. } => projection,
        }
    }
}

struct LockedState {
    store: RunStore,
    recovered: RecoveredRun,
    native_goal_mismatch: bool,
}

pub fn on_session_start(enrollment: &Enrollment) -> Result<KeeperDecision, String> {
    decision_from_state(reconcile_provider(enrollment)?)
}

pub fn on_observation(enrollment: &Enrollment) -> Result<KeeperDecision, String> {
    decision_from_state(reconcile_provider(enrollment)?)
}

mod prompt;
mod reconcile;
pub use prompt::*;
use reconcile::*;

fn commit_resolution(
    state: &mut LockedState,
    resolution: InspectResolution,
    cursor: &SourceCursor,
) -> Result<(), String> {
    crate::keeper_metrics::record(&state.store, &resolution.metadata)?;
    let accepted_events = state.recovered.history.records().len();
    for record in &resolution.history.records()[accepted_events..] {
        state
            .store
            .append_pending(record.clone())
            .map_err(|error| error.to_string())?;
    }
    if let Some(goal_change) = &resolution.goal_change {
        let proposal = PendingGoalChange::new(goal_change, cursor.digest())
            .map_err(|error| error.to_string())?;
        GoalChangeStore::open(&state.store)
            .and_then(|store| store.persist(&proposal))
            .map_err(|error| error.to_string())?;
    }
    state
        .store
        .commit_projection_with_source_cursor(&resolution.projection, cursor)
        .map_err(|error| error.to_string())?;
    state.recovered.history = resolution.history;
    state.recovered.projection = resolution.projection;
    state.recovered.pending_delta.clear();
    state.recovered.source_cursor = Some(cursor.clone());
    Ok(())
}

fn open_state(enrollment: &Enrollment) -> Result<LockedState, String> {
    let store = RunStore::open_default(enrollment.repository(), enrollment.run_id())
        .map_err(|error| error.to_string())?;
    let recovered = store.recover().map_err(|error| error.to_string())?;
    Ok(LockedState {
        store,
        recovered,
        native_goal_mismatch: false,
    })
}

fn decision_from_state(state: LockedState) -> Result<KeeperDecision, String> {
    if let Some(reason) = blocker(&state)? {
        Ok(KeeperDecision::Blocked {
            projection: state.recovered.projection,
            reason,
        })
    } else {
        Ok(KeeperDecision::Ready(state.recovered.projection))
    }
}

fn blocker(state: &LockedState) -> Result<Option<String>, String> {
    if state.native_goal_mismatch {
        return Ok(Some(
            "the Codex native goal differs from the accepted Driftctl goal and requires operator approval"
                .to_owned(),
        ));
    }
    if state.recovered.projection.continuation_blocked() {
        return Ok(Some(
            "active intent contains an unresolved conflict or exceeded its projection budget"
                .to_owned(),
        ));
    }
    let proposal_store = GoalChangeStore::open(&state.store).map_err(|error| error.to_string())?;
    if proposal_store
        .load_pending()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(Some(
            "an overall goal change requires an explicit operator decision".to_owned(),
        ));
    }
    Ok(None)
}

fn prompt_record_id(turn_id: &str) -> Result<String, String> {
    if turn_id.trim().is_empty()
        || turn_id.len() > 16 * 1024
        || turn_id.chars().any(char::is_control)
    {
        return Err("KEEPER_CODEX_USERPROMPTSUBMIT_REQUIRES_A_VALID_TURN_ID stage=keeper: Codex UserPromptSubmit requires a valid turn_id".to_owned());
    }
    Ok(format!("{turn_id}:0"))
}
