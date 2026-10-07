use super::*;
#[test]
fn native_receipt_diagnostic_only_names_actions_the_observer_can_perform() {
    let text = missing_receipt().to_string();
    assert!(text.starts_with("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery"));
    assert!(text.contains("acknowledge") && text.contains("remain idle"));
    assert!(!text.contains("inspect private") && !text.contains("review/delivery-gap.json"));
}
