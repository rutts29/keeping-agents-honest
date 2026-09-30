use super::*;

#[test]
fn explicit_correction_can_restate_goal_summary_with_user_sources() {
    let (history, projection) = accepted_state();
    let delta = delta("u3", "Correction: preserve the safer behavior instead.");
    let mut op = operation("supersede", "u3");
    op["key"] = json!("safer-behavior");
    op["target_intent_id"] = json!("intent-existing");
    op["text"] = json!("Preserve the safer behavior");
    let mut proposed = proposal(&history, &projection, "supersession", "u3", json!([op]));
    proposed.proposed_goal = Some(IncrementalGoalProposal {
        text: "Ship the inspector preserving safer behavior".into(),
        source_record_ids: vec!["u3".into()],
    });
    let (next, _, goal) = validate_incremental_and_project(
        &history,
        &projection,
        &delta,
        proposed,
        ProjectionConfig::default(),
    )
    .unwrap();
    assert_eq!(
        next.goal().text,
        history.goal().text,
        "the compiler returns a source-linked revision candidate"
    );
    let goal = goal.unwrap();
    assert_eq!(goal.source_refs[0].role, SourceRole::User);
    assert_eq!(
        goal.proposed_goal,
        "Ship the inspector preserving safer behavior"
    );
}
