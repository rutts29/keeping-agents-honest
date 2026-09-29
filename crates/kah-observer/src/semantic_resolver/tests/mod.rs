use super::*;
use kah_core::intent_history::{EvidenceRef, IntentLifecycle, SourceProvider, SourceRole};
pub(super) fn user_source(record: &str, digest: &str) -> SourceRef {
    SourceRef::new(
        SourceProvider::Codex,
        "private-session",
        record,
        SourceRole::User,
        digest,
    )
}

pub(super) fn accepted_state() -> (History, ActiveProjection) {
    let prior_record = BundleRecord::new(
        "u2",
        SourceRole::User,
        "RAW_PRIOR_SECRET original transcript wording",
    )
    .unwrap();
    let mut history = History::new(GoalRevision::new(
        1,
        "Ship the inspector",
        vec![user_source("u1", "goal-digest")],
    ))
    .unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "intent-existing",
                IntentKind::Constraint,
                "Keep synthesized behavior",
                vec![user_source("u2", prior_record.content_digest())],
            ),
            approval: None,
        })
        .unwrap();
    let projection = ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap();
    (history, projection)
}

pub(super) fn delta(record: &str, content: &str) -> NeutralSessionBundle {
    NeutralSessionBundle::from_records(
        SourceProvider::Codex,
        "private-session",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec![BundleRecord::new(record, SourceRole::User, content).unwrap()],
    )
    .unwrap()
}

pub(super) fn proposal(
    history: &History,
    projection: &ActiveProjection,
    classification: &str,
    source_record: &str,
    operations: Value,
) -> IncrementalProjectionProposal {
    serde_json::from_value(json!({
        "schema_version":1,
        "base_projection_revision":projection.revision,
        "base_event_sequence":history.records().last().unwrap().sequence,
        "classification":classification,
        "accounted_active_intent_ids":projection_intent_ids(projection),
        "accounted_source_record_ids":[source_record],
        "operations":operations,
    }))
    .unwrap()
}

pub(super) fn operation(name: &str, source_record: &str) -> Value {
    json!({
        "operation":name,
        "key":"",
        "kind":"constraint",
        "text":"",
        "target_intent_id":"",
        "intent_ids":[],
        "evidence_id":"",
        "reason":"",
        "source_record_ids":[source_record],
        "alternatives":[],
    })
}

mod assessment;
mod backend;
mod continuity;
mod operations;
mod privacy;
mod prompts;
mod provenance;
mod schema;
