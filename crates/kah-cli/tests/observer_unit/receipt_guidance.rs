use super::*;
#[test]
fn native_receipt_diagnostic_only_names_actions_the_observer_can_perform() {
    let text = missing_receipt().to_string();
    assert!(text.starts_with("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery"));
    assert!(text.contains("acknowledge") && text.contains("remain idle"));
    assert!(!text.contains("inspect private") && !text.contains("review/delivery-gap.json"));
}

#[test]
fn receipt_marker_matches_the_owned_preparation_or_batch_prompt_not_its_parent() {
    let root = "review-parent";
    let child = "review-parent-v8-decision";
    let batch = "review-parent-batch-digest";
    assert_eq!(
        prompt_marker(
            &format!("kah review request {child} (Preparation). Read jobs."),
            root,
            child
        )
        .unwrap(),
        format!("kah review request {child} ")
    );
    assert_eq!(
        prompt_marker(
            &format!("kah review request {batch} (Preparation). Read jobs."),
            root,
            batch
        )
        .unwrap(),
        format!("kah review request {batch} ")
    );
    assert_eq!(
        prompt_marker(
            &format!("kah review request {root} (Checkpoint)."),
            root,
            child
        )
        .unwrap(),
        format!("kah review request {root} ")
    );
    assert!(
        prompt_marker(
            "kah review request review-other (Preparation).",
            root,
            child
        )
        .is_err()
    );
}
