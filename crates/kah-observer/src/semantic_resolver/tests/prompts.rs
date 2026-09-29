use super::*;

#[test]
fn initial_prompt_explains_how_to_shape_a_new_ambiguous_choice() {
    let bundle = delta("item-1", "I have not chosen between JSON and YAML");
    let rendered = prompt(&bundle, None).unwrap();
    let document: Value = serde_json::from_str(&rendered).unwrap();
    let instructions = document["instructions"].as_str().unwrap();

    assert!(instructions.contains("conflict target_key must be empty"));
    assert!(instructions.contains("intent_keys must name earlier add or supersede keys"));
    assert!(instructions.contains("currently active add or supersede key"));
    assert!(instructions.contains("first add one neutral unresolved-choice clause"));
    assert!(instructions.contains("remains unresolved at the end"));
    assert!(instructions.contains("do not emit a transient conflict"));
}

#[test]
fn initial_prompt_preserves_a_known_native_goal_without_false_rewording() {
    let bundle = NeutralSessionBundle::from_records_with_native_goal(
        SourceProvider::Codex,
        "private-session",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        NativeGoal::known("Keep the accepted native objective exact").unwrap(),
        vec![BundleRecord::new("u1", SourceRole::User, "Continue the same objective").unwrap()],
    )
    .unwrap();

    let rendered = prompt(&bundle, None).unwrap();
    let document: Value = serde_json::from_str(&rendered).unwrap();
    let instructions = document["instructions"].as_str().unwrap();

    assert!(instructions.contains("copy its text verbatim"));
    assert!(instructions.contains("explicit user record changes the overall objective"));
}

#[test]
fn incremental_prompt_contains_projection_and_delta_but_no_history_or_old_raw_text() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "RAW_NEW_DELTA add another invariant");

    let prompt = incremental_prompt(&history, &projection, &delta, None).unwrap();

    assert!(prompt.contains("Keep synthesized behavior"));
    assert!(prompt.contains("RAW_NEW_DELTA"));
    assert!(!prompt.contains("RAW_PRIOR_SECRET"));
    assert!(!prompt.contains("immutable_history"));
    assert!(!prompt.contains("run_started"));
}

#[test]
fn incremental_prompt_does_not_infer_goal_change_from_session_wide_constraints() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "From now on, end every reply with a marker");

    let rendered = incremental_prompt(&history, &projection, &delta, None).unwrap();
    let document: Value = serde_json::from_str(&rendered).unwrap();
    let instructions = document["instructions"].as_str().unwrap();

    assert!(instructions.contains("explicitly replaces the overall goal, objective, or task"));
    assert!(instructions.contains("Never infer a goal change from `from now on`"));
    assert!(instructions.contains("output-format, style, tool, scope, validation"));
}

#[test]
fn incremental_repair_names_the_validator_failure_and_conflict_shape() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Choose between the incompatible active constraints");

    let rendered =
        incremental_prompt(&history, &projection, &delta, Some("conflict_shape")).unwrap();
    let document: Value = serde_json::from_str(&rendered).unwrap();
    let instructions = document["instructions"].as_str().unwrap();

    assert_eq!(document["mode"], "repair");
    assert_eq!(document["previous_failure"], "conflict_shape");
    assert!(instructions.contains("correct the named previous_failure"));
    assert!(instructions.contains("An add operation uses a non-empty key"));
    assert!(instructions.contains("empty target_intent_id, intent_ids, evidence_id, reason"));
    assert!(instructions.contains("A withdraw operation uses an empty key and text"));
    assert!(instructions.contains("empty target_intent_id, evidence_id, and reason"));
    assert!(instructions.contains("Every conflict alternative"));
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
