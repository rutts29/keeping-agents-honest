use super::*;

pub fn sanitized_json(
    resolution: &InspectResolution,
    imported_user_records: usize,
    source_digest: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&sanitized_value(
        resolution,
        imported_user_records,
        source_digest,
    ))
}

pub fn sanitized_human(
    resolution: &InspectResolution,
    imported_user_records: usize,
    source_digest: &str,
) -> String {
    let status = if resolution.continuation_blocked() {
        "blocked"
    } else {
        "usable"
    };
    let mut lines = vec![
        format!("status: {status}"),
        "provider: codex".to_owned(),
        format!("imported user records: {imported_user_records}"),
        format!("source digest: {source_digest}"),
        format!(
            "compactor: {} ({})",
            resolution.metadata.model, resolution.metadata.reasoning
        ),
        format!("calls: {}", resolution.metadata.calls),
        format!("elapsed ms: {}", resolution.metadata.elapsed_ms),
        format!("native goal state: {}", resolution.native_goal.state),
        format!(
            "native goal conflicts with projection: {}",
            resolution.native_goal.conflicts_with_projection
        ),
        format!("goal: {}", resolution.projection.goal.text),
    ];
    for item in &resolution.projection.preserve {
        lines.push(format!("preserve [{}]: {}", item.id, item.text));
    }
    for item in &resolution.projection.frontier {
        lines.push(format!("frontier [{}]: {}", item.id, item.text));
    }
    for item in &resolution.projection.validation {
        lines.push(format!("validation [{}]: {}", item.id, item.text));
    }
    for conflict in &resolution.projection.conflicts {
        lines.push(format!("conflict [{}]: unresolved", conflict.id));
    }
    if let Some(reason) = &resolution.projection.overflow.reason {
        lines.push(format!("overflow: {reason}"));
    }
    if let Some(proposal) = &resolution.goal_change {
        lines.push(format!(
            "goal change pending operator approval: {}",
            proposal.proposed_goal
        ));
    }
    lines.join("\n")
}

pub(super) fn sanitized_value(
    resolution: &InspectResolution,
    imported_user_records: usize,
    source_digest: &str,
) -> Value {
    let projection = &resolution.projection;
    let history = resolution
        .history
        .intents()
        .values()
        .map(public_item)
        .collect::<Vec<_>>();
    let conflicts = public_conflicts(projection);
    let blockers = public_blockers(resolution);
    let status = if blockers.is_empty() {
        "usable"
    } else {
        "blocked"
    };
    json!({
        "schema_version":1,
        "provider":"codex",
        "status":status,
        "source":{
            "digest":source_digest,
            "imported_user_records":imported_user_records,
            "head_record_id":projection.source_head.record,
        },
        "projection":{
            "schema_version":projection.schema_version,
            "revision":projection.revision,
            "goal":{
                "revision":projection.goal.revision,
                "text":projection.goal.text,
                "source_record_ids":source_record_ids(&projection.goal.source_refs),
            },
            "preserve":projection.preserve.iter().map(public_item).collect::<Vec<_>>(),
            "frontier":projection.frontier.iter().map(public_item).collect::<Vec<_>>(),
            "validation":projection.validation.iter().map(public_item).collect::<Vec<_>>(),
            "conflicts":conflicts,
            "overflow":projection.overflow,
            "generated_by":projection.generated_by,
        },
        "history":history,
        "blockers":blockers,
        "resolver":resolution.metadata,
        "native_goal":{
            "state":resolution.native_goal.state,
            "conflicts_with_projection":resolution.native_goal.conflicts_with_projection,
        },
        "goal_change":resolution.goal_change.as_ref().map(|proposal| json!({
            "proposed_goal":proposal.proposed_goal,
            "source_record_ids":source_record_ids(&proposal.source_refs),
            "base_projection_revision":proposal.base_projection_revision,
            "base_event_sequence":proposal.base_event_sequence,
            "needs_operator_approval":true,
        })),
    })
}

pub(super) fn public_item(item: &IntentItem) -> Value {
    json!({
        "id":item.id,
        "kind":item.kind,
        "text":item.text,
        "lifecycle":item.lifecycle,
        "evidence_state":item.evidence_state,
        "source_record_ids":source_record_ids(&item.introduced_by),
        "changed_by_record_ids":source_record_ids(&item.changed_by),
        "supersedes":item.supersedes,
    })
}

pub(super) fn source_record_ids(sources: &[SourceRef]) -> Vec<&str> {
    sources
        .iter()
        .map(|source| source.record.as_str())
        .collect()
}

fn public_conflicts(projection: &ActiveProjection) -> Vec<Value> {
    projection
        .conflicts
        .iter()
        .map(|conflict| {
            json!({
                "id": conflict.id,
                "intent_ids": conflict.intent_ids,
                "alternatives": conflict.alternatives.iter().map(|alternative| json!({
                    "id": alternative.id,
                    "text": alternative.text,
                    "source_record_ids": source_record_ids(&alternative.source_refs),
                })).collect::<Vec<_>>(),
                "source_record_ids": source_record_ids(&conflict.source_refs),
            })
        })
        .collect::<Vec<_>>()
}

fn public_blockers(resolution: &InspectResolution) -> Vec<Value> {
    let projection = &resolution.projection;
    let mut blockers = projection
        .conflicts
        .iter()
        .map(|conflict| {
            json!({
                "kind":"conflict",
                "id":conflict.id,
                "reason":"unresolved semantic conflict blocks continuation",
                "source_record_ids":source_record_ids(&conflict.source_refs),
            })
        })
        .collect::<Vec<_>>();
    if projection.is_overflowed() {
        blockers.push(json!({
            "kind":"projection_overflow",
            "id":null,
            "reason":projection.overflow.reason,
            "source_record_ids":[],
        }));
    }
    if resolution.native_goal.conflicts_with_projection {
        blockers.push(json!({
            "kind":"native_goal_conflict",
            "id":null,
            "reason":"observed native goal differs from accepted projected goal; only an approved child migration may continue",
            "source_record_ids":[],
        }));
    }
    if let Some(proposal) = &resolution.goal_change {
        blockers.push(json!({
            "kind":"goal_change_pending",
            "id":null,
            "reason":"proposed goal change requires operator authority",
            "source_record_ids":source_record_ids(&proposal.source_refs),
        }));
    }
    blockers
}
