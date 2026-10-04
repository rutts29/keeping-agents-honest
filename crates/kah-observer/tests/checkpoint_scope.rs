use kah_observer::review_evidence::{Document, Report};
use serde_json::json;

#[test]
fn named_components_and_phase_have_a_verified_source_anchor() {
    let doc = Document::new(
        "plan",
        "plan",
        "docs/PLAN.md",
        "Phase 2 integrates parser and renderer.",
    );
    let value = json!({"request_id":"r","findings":[{"topic":"integration","claim":"Review parser and renderer","checkpoint_scope":{"phase":"Phase 2","components":["parser","renderer"],"authorization_citation":0},"citations":[{"source":"plan","digest":doc.digest,"quote":doc.text}]}]});
    let report: Report = serde_json::from_value(value.clone()).unwrap();
    let verified = report.verify("r", std::slice::from_ref(&doc)).unwrap();
    assert_eq!(
        verified[0]["checkpoint_scope"]["components"],
        json!(["parser", "renderer"])
    );
    let mut invalid = value;
    invalid["findings"][0]["checkpoint_scope"]["authorization_citation"] = 1.into();
    let report: Report = serde_json::from_value(invalid).unwrap();
    assert!(
        report
            .verify("r", &[doc])
            .unwrap_err()
            .starts_with("REVIEW_CHECKPOINT_SCOPE")
    );
}
