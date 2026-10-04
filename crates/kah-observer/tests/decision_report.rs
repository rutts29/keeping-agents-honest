use kah_observer::{
    decision_units::ReviewUnits,
    review_evidence::{Document, Report},
};
use serde_json::json;

#[test]
fn native_report_accepts_typed_units_and_requires_review_of_existing_ledger_decisions() {
    let ledger = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "Preserve exact spacing. Normalize spacing.",
    );
    let units = json!({"requirements":[{"id":"format","name":"Output format","dimension":"intent","authority_items":["goal"],"citations":[{"source":"ledger","digest":ledger.digest,"quote":ledger.text}],"components":[],"checks":[],"judgment":{"kind":"needs_user","reason":"contradictory_instructions","summary":"The two current requirements conflict.","question":"Which spacing rule applies?","options":["Exact","Normalized"],"evidence_citations":[0],"ledger_review":{"reasoning":"Both current rules are present; neither supersedes the other.","citations":[0]}}}],"proportion":null});
    let report: Report = serde_json::from_value(json!({"request_id":"r","findings":[{"topic":"intent","claim":"Format is genuinely unresolved","citations":[{"source":"ledger","digest":ledger.digest,"quote":ledger.text}]}],"decision_units":units})).unwrap();
    let value = report
        .verify_units(std::slice::from_ref(&ledger), &["goal".into()])
        .unwrap()
        .unwrap();
    assert_eq!(
        value["requirements"][0]["judgment"]["ledger_review"]["citations"],
        json!([0])
    );
    let mut unchecked = serde_json::to_value(report.decision_units.unwrap()).unwrap();
    unchecked["requirements"][0]["judgment"]["ledger_review"]["citations"] = json!([]);
    assert!(
        serde_json::from_value::<ReviewUnits>(unchecked)
            .unwrap()
            .verify(&[ledger], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_LEDGER_CHECK")
    );
}
