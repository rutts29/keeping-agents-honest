use kah_observer::review_evidence::Report;
use serde_json::json;
#[test]
fn thinker_can_explain_remaining_risks_without_supplying_probabilities() {
    let value = json!({"request_id":"review-risk","findings":[],"verification_risks":[{
        "requirement_id":"provider-integration",
        "unverified":"The second provider's interaction could not be independently reproduced.",
        "acceptance_risk":"Acceptance may advance with a provider-specific integration defect."
    }]});
    let report: Report = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(report.verification_risks.unwrap().len(), 1);
    let mut invalid = value;
    invalid["verification_risks"][0]["probability"] = json!(0.99);
    assert!(serde_json::from_value::<Report>(invalid).is_err());
    assert!(Report::schema()["properties"]["verification_risks"].is_object());
}
