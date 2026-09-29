use super::*;

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
