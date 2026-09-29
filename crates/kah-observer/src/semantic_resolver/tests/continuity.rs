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
