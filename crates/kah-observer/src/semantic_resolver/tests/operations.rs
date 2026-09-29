use super::*;

#[test]
fn initial_add_ignores_fields_that_have_no_add_semantics() {
    let bundle = delta("item-1", "Add one bounded retry");
    let proposal: ProjectionProposal = serde_json::from_value(json!({
        "schema_version": 1,
        "goal": {
            "text": "Add one bounded retry",
            "source_record_ids": ["item-1"]
        },
        "accounted_source_record_ids": ["item-1"],
        "operations": [{
            "operation": "add",
            "key": "retry_once",
            "kind": "constraint",
            "text": "Retry a transient failure exactly once",
            "target_key": "ignored_for_add",
            "intent_keys": ["ignored_for_add"],
            "source_record_ids": ["item-1"],
            "alternatives": []
        }]
    }))
    .unwrap();

    let history = validate_proposal(&bundle, proposal).unwrap();
    assert!(
        history
            .intents()
            .values()
            .any(|intent| intent.text == "Retry a transient failure exactly once")
    );
}

#[test]
fn initial_supersede_ignores_intent_keys_that_have_no_supersede_semantics() {
    let bundle = delta("item-1", "Use the final duplicate policy instead");
    let proposal: ProjectionProposal = serde_json::from_value(json!({
        "schema_version": 1,
        "goal": {
            "text": "Paginate catalog results",
            "source_record_ids": ["item-1"]
        },
        "accounted_source_record_ids": ["item-1"],
        "operations": [
            {
                "operation": "add",
                "key": "duplicates.last_seen",
                "kind": "invariant",
                "text": "Keep the last duplicate",
                "target_key": "",
                "intent_keys": [],
                "source_record_ids": ["item-1"],
                "alternatives": []
            },
            {
                "operation": "supersede",
                "key": "duplicates.first_seen",
                "kind": "invariant",
                "text": "Keep the first duplicate",
                "target_key": "duplicates.last_seen",
                "intent_keys": ["duplicates.last_seen"],
                "source_record_ids": ["item-1"],
                "alternatives": []
            }
        ]
    }))
    .unwrap();

    let history = validate_proposal(&bundle, proposal).unwrap();
    assert!(history.intents().values().any(|intent| {
        intent.text == "Keep the last duplicate" && intent.lifecycle == IntentLifecycle::Superseded
    }));
    assert!(history.intents().values().any(|intent| {
        intent.text == "Keep the first duplicate" && intent.lifecycle == IntentLifecycle::Active
    }));
}

#[test]
fn incremental_add_appends_to_history_and_keeps_existing_id() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "RAW_NEW_DELTA add another invariant");
    let proposal: IncrementalProjectionProposal = serde_json::from_value(json!({
        "schema_version": 1,
        "base_projection_revision": projection.revision,
        "base_event_sequence": history.records().last().unwrap().sequence,
        "classification": "additive",
        "accounted_active_intent_ids": ["intent-existing"],
        "accounted_source_record_ids": ["u3"],
        "operations": [{
            "operation": "add",
            "key": "new-invariant",
            "kind": "invariant",
            "text": "Keep the new invariant",
            "target_intent_id": "",
            "intent_ids": [],
            "evidence_id": "",
            "reason": "",
            "source_record_ids": ["u3"],
            "alternatives": []
        }]
    }))
    .unwrap();

    let (next_history, next_projection, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposal,
        ProjectionConfig::default(),
    )
    .unwrap();
    assert!(goal_change.is_none());

    assert_eq!(next_history.records().len(), history.records().len() + 1);
    assert_eq!(
        next_history.intent("intent-existing").unwrap().id.as_str(),
        "intent-existing"
    );
    assert!(next_projection.frontier.iter().any(|item| {
        item.id != IntentId::new("intent-existing") && item.text == "Keep the new invariant"
    }));
}

#[test]
fn incremental_supersession_preserves_old_id_as_inactive_and_adds_replacement() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Replace the existing constraint explicitly");
    let mut supersede = operation("supersede", "u3");
    supersede["key"] = json!("replacement");
    supersede["text"] = json!("Use the replacement behavior");
    supersede["target_intent_id"] = json!("intent-existing");

    let (next, projected, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposal(
            &history,
            &projection,
            "supersession",
            "u3",
            json!([supersede]),
        ),
        ProjectionConfig::default(),
    )
    .unwrap();
    assert!(goal_change.is_none());

    assert_eq!(
        next.intent("intent-existing").unwrap().lifecycle,
        IntentLifecycle::Superseded
    );
    assert!(
        !projected
            .frontier
            .iter()
            .any(|item| item.id.as_str() == "intent-existing")
    );
    assert!(projected.frontier.iter().any(|item| {
        item.text == "Use the replacement behavior"
            && item.supersedes == [IntentId::new("intent-existing")]
    }));
}

#[test]
fn incremental_withdrawal_retains_terminal_history_without_projecting_item() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Withdraw the existing constraint explicitly");
    let mut withdraw = operation("withdraw", "u3");
    withdraw["target_intent_id"] = json!("intent-existing");

    let (next, projected, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposal(&history, &projection, "withdrawal", "u3", json!([withdraw])),
        ProjectionConfig::default(),
    )
    .unwrap();
    assert!(goal_change.is_none());

    assert_eq!(
        next.intent("intent-existing").unwrap().lifecycle,
        IntentLifecycle::Withdrawn
    );
    assert!(projection_intent_ids(&projected).is_empty());
}

#[test]
fn incremental_ambiguity_raises_a_source_linked_blocking_conflict() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Ambiguous choice between keeping or replacing it");
    let mut conflict = operation("conflict", "u3");
    conflict["key"] = json!("behavior-choice");
    conflict["text"] = json!("Behavior choice is ambiguous");
    conflict["intent_ids"] = json!(["intent-existing"]);
    conflict["alternatives"] = json!([
        {"key":"keep","text":"Keep it","source_record_ids":["u3"]},
        {"key":"replace","text":"Replace it","source_record_ids":["u3"]}
    ]);

    let (next, projected, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposal(&history, &projection, "conflict", "u3", json!([conflict])),
        ProjectionConfig::default(),
    )
    .unwrap();
    assert!(goal_change.is_none());

    assert_eq!(
        next.intent("intent-existing").unwrap().lifecycle,
        IntentLifecycle::Conflicted
    );
    assert_eq!(projected.conflicts.len(), 1);
    assert!(projected.continuation_blocked());
    assert_eq!(projected.conflicts[0].source_refs[0].record, "u3");
}

#[test]
fn incremental_goal_change_is_proposal_only_and_blocks_until_operator_authority() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Replace the overall objective with a safe migration");
    let proposal: IncrementalProjectionProposal = serde_json::from_value(json!({
        "schema_version":1,
        "base_projection_revision":projection.revision,
        "base_event_sequence":history.records().last().unwrap().sequence,
        "classification":"goal_change",
        "accounted_active_intent_ids":["intent-existing"],
        "accounted_source_record_ids":["u3"],
        "operations":[],
        "proposed_goal":{
            "text":"Ship the safe migration",
            "source_record_ids":["u3"]
        }
    }))
    .unwrap();

    let (next, projected, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposal,
        ProjectionConfig::default(),
    )
    .unwrap();

    assert_eq!(next, history);
    assert_eq!(projected.rendered_prompt(), projection.rendered_prompt());
    let goal_change = goal_change.expect("pending goal change");
    assert_eq!(goal_change.proposed_goal, "Ship the safe migration");
    assert_eq!(goal_change.base_projection_revision, projection.revision);
    let resolution = InspectResolution {
        history: next,
        projection: projected,
        metadata: metadata(
            CompactorConfig::default(),
            1,
            0,
            ResolverUsage::default(),
            None,
            &[],
        ),
        native_goal: observe_native_goal(&NativeGoal::Absent, &history.goal().text),
        goal_change: Some(goal_change),
    };
    assert!(resolution.continuation_blocked());
    let public = sanitized_value(&resolution, 1, delta.source().digest());
    assert_eq!(public["status"], "blocked");
    assert_eq!(public["goal_change"]["needs_operator_approval"], true);
}

#[test]
fn incremental_reopen_invalidates_existing_evidence_without_changing_intent_id() {
    let (mut history, _) = accepted_state();
    history
        .append(Event::EvidenceAttached {
            intent_id: IntentId::new("intent-existing"),
            evidence: EvidenceRef::new(
                "evidence-1",
                "Prior verification",
                vec![SourceRef::new(
                    SourceProvider::Codex,
                    "private-session",
                    "a1",
                    SourceRole::Assistant,
                    "evidence-digest",
                )],
            ),
        })
        .unwrap();
    let projection = ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap();
    let delta = delta("u3", "The prior verification is no longer valid");
    let mut reopen = operation("reopen", "u3");
    reopen["target_intent_id"] = json!("intent-existing");
    reopen["evidence_id"] = json!("evidence-1");
    reopen["reason"] = json!("New steering invalidates prior verification");

    let (next, projected, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposal(&history, &projection, "reopen", "u3", json!([reopen])),
        ProjectionConfig::default(),
    )
    .unwrap();
    assert!(goal_change.is_none());

    assert_eq!(
        next.intent("intent-existing").unwrap().id.as_str(),
        "intent-existing"
    );
    assert!(
        projected
            .frontier
            .iter()
            .any(|item| item.id.as_str() == "intent-existing")
    );
}
