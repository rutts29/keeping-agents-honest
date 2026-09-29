use super::*;

#[test]
fn drift_must_reference_real_actions_and_never_creates_user_authority() {
    let actions = vec![json!({"id":"action-1","kind":"fileChange","evidence":"wrong file"})];
    assert!(
        validate_assessment(
            json!({"state":"material_drift","reason":"wrong file","action_ids":["action-1"]}),
            &actions
        )
        .is_ok()
    );
    assert!(
        validate_assessment(
            json!({"state":"material_drift","reason":"wrong file","action_ids":["invented"]}),
            &actions
        )
        .is_err()
    );
    assert!(
        validate_assessment(
            json!({"state":"material_drift","reason":"wrong file","action_ids":[]}),
            &actions
        )
        .is_err()
    );
    assert!(
        validate_assessment(
            json!({"state":"aligned","reason":"matches intent","action_ids":[]}),
            &actions
        )
        .is_ok()
    );
}
