use super::*;

#[test]
fn incremental_resolution_repairs_once_and_records_native_goal_conflict_without_revising_goal() {
    let (history, projection) = accepted_state();
    let delta = NeutralSessionBundle::from_records_with_native_goal(
        SourceProvider::Codex,
        "private-session",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        NativeGoal::known("Different native goal").unwrap(),
        vec![BundleRecord::new("u3", SourceRole::User, "Add another invariant").unwrap()],
    )
    .unwrap();
    let mut add = operation("add", "u3");
    add["key"] = json!("repair-add");
    add["kind"] = json!("invariant");
    add["text"] = json!("Keep repaired behavior");
    let valid = serde_json::to_string(&proposal(
        &history,
        &projection,
        "additive",
        "u3",
        json!([add]),
    ))
    .unwrap();
    let mut attempt = 0_u8;

    let resolution = resolve_incremental_with(
        &history,
        &projection,
        &delta,
        CompactorConfig::default(),
        ProjectionConfig::default(),
        |repair_failure| {
            attempt += 1;
            assert_eq!(repair_failure.is_some(), attempt == 2);
            if attempt == 2 {
                assert_eq!(repair_failure, Some("proposal_deserialization"));
            }
            Ok(CodexCall {
                final_message: if attempt == 1 {
                    "not-json".to_owned()
                } else {
                    valid.clone()
                },
                usage: ResolverUsage::default(),
                artifact_id: format!("artifact-{attempt}"),
                trajectory: Vec::new(),
            })
        },
    )
    .unwrap();

    assert_eq!(resolution.metadata.calls, 2);
    assert_eq!(
        resolution.metadata.prompt_schema_version,
        INCREMENTAL_PROMPT_SCHEMA_VERSION
    );
    assert!(resolution.native_goal.conflicts_with_projection);
    assert!(resolution.continuation_blocked());
    assert_eq!(
        resolution.native_goal.text_private.as_deref(),
        Some("Different native goal")
    );
    assert_eq!(resolution.history.goal().text, "Ship the inspector");
}

#[test]
fn incremental_validation_rejects_stale_bases() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Add another invariant");
    let mut add = operation("add", "u3");
    add["key"] = json!("new-item");
    add["text"] = json!("Keep another invariant");

    let mut stale = proposal(
        &history,
        &projection,
        "additive",
        "u3",
        json!([add.clone()]),
    );
    stale.base_event_sequence += 1;
    assert!(matches!(
        validate_incremental_and_project(
            &history,
            &projection,
            &delta,
            stale,
            ProjectionConfig::default()
        ),
        Err(ValidationFailure::StaleBase)
    ));

    let mut stale_revision = proposal(
        &history,
        &projection,
        "additive",
        "u3",
        json!([add.clone()]),
    );
    stale_revision.base_projection_revision += 1;
    assert!(matches!(
        validate_incremental_and_project(
            &history,
            &projection,
            &delta,
            stale_revision,
            ProjectionConfig::default()
        ),
        Err(ValidationFailure::StaleBase)
    ));
}

#[test]
fn incremental_rejects_missing_intents_and_invented_sources() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Add another invariant");
    let mut add = operation("add", "u3");
    add["key"] = json!("new-item");
    add["text"] = json!("Keep another invariant");
    let mut missing = proposal(
        &history,
        &projection,
        "additive",
        "u3",
        json!([add.clone()]),
    );
    missing.accounted_active_intent_ids.clear();
    assert!(matches!(
        validate_incremental_and_project(
            &history,
            &projection,
            &delta,
            missing,
            ProjectionConfig::default()
        ),
        Err(ValidationFailure::ActiveAccounting)
    ));

    add["source_record_ids"] = json!(["invented"]);
    let invented = proposal(&history, &projection, "additive", "u3", json!([add]));
    assert!(matches!(
        validate_incremental_and_project(
            &history,
            &projection,
            &delta,
            invented,
            ProjectionConfig::default()
        ),
        Err(ValidationFailure::Source)
    ));
}

#[test]
fn incremental_rejects_replayed_sources_and_projection_overflow() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Add another invariant");
    let repeated_source = self::delta("u2", "Attempt to reuse a prior source ID");
    let mut repeated_add = operation("add", "u2");
    repeated_add["key"] = json!("repeated-source");
    repeated_add["text"] = json!("This must not commit");
    let repeated = proposal(
        &history,
        &projection,
        "additive",
        "u2",
        json!([repeated_add]),
    );
    assert!(matches!(
        validate_incremental_and_project(
            &history,
            &projection,
            &repeated_source,
            repeated,
            ProjectionConfig::default()
        ),
        Err(ValidationFailure::Source)
    ));

    let mut valid_add = operation("add", "u3");
    valid_add["key"] = json!("new-item");
    valid_add["text"] = json!("Keep another invariant");
    let overflow = proposal(&history, &projection, "additive", "u3", json!([valid_add]));
    assert!(matches!(
        validate_incremental_and_project(
            &history,
            &projection,
            &delta,
            overflow,
            ProjectionConfig::new(1)
        ),
        Err(ValidationFailure::Projection)
    ));
}

#[test]
fn repeated_two_step_compaction_keeps_each_prompt_to_projection_plus_current_delta() {
    let (history, projection) = accepted_state();
    let first_delta = delta("u3", "RAW_FIRST_DELTA private transcript wording");
    let mut first_add = operation("add", "u3");
    first_add["key"] = json!("first-add");
    first_add["text"] = json!("Synthesized first addition");
    let (next_history, next_projection, goal_change) = validate_incremental_and_project(
        &history,
        &projection,
        &first_delta,
        proposal(&history, &projection, "additive", "u3", json!([first_add])),
        ProjectionConfig::default(),
    )
    .unwrap();
    assert!(goal_change.is_none());
    let second_delta = delta("u4", "RAW_SECOND_DELTA current wording");

    let second_prompt =
        incremental_prompt(&next_history, &next_projection, &second_delta, None).unwrap();

    assert!(second_prompt.contains("Synthesized first addition"));
    assert!(second_prompt.contains("RAW_SECOND_DELTA"));
    assert!(!second_prompt.contains("RAW_FIRST_DELTA"));
    assert!(!second_prompt.contains("RAW_PRIOR_SECRET"));
    let payload: Value = serde_json::from_str(&second_prompt).unwrap();
    assert!(payload.get("active_projection").is_some());
    assert!(payload.get("delta_records").is_some());
    assert!(payload.get("source_map").is_some());
    assert!(payload.get("history").is_none());
}

#[test]
fn oversized_delta_is_rejected_before_a_provider_prompt_can_be_built() {
    let (history, projection) = accepted_state();
    let oversized = delta("u3", &"x".repeat(MAX_INCREMENTAL_DELTA_PROMPT_BYTES));

    assert!(matches!(
        incremental_prompt(&history, &projection, &oversized, None),
        Err(ValidationFailure::DeltaBound)
    ));
}
