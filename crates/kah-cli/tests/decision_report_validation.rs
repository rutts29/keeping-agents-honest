use kah_cli::observer_review::{
    decisions,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::{Document, Report};
use serde_json::json;

#[test]
fn native_submission_verifies_every_protected_item_before_acknowledging_units() {
    let doc = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "Integrate both components. Keep notes private.",
    );
    let request = Request {
        id: "review-unit".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({"goal":"Integrate both components.","items":[{"id":"private-notes","text":"Keep notes private."}]}),
        trigger: json!({}),
        documents: vec![doc.clone()],
        created_ms: 0,
    };
    let value = json!({"request_id":"review-unit","findings":[{"topic":"intent","claim":"Integration only","citations":[{"source":"ledger","digest":doc.digest,"quote":doc.text}]}],"decision_units":{"requirements":[{"id":"integration","name":"Components integrate","dimension":"integration","authority_items":["goal"],"citations":[{"source":"ledger","digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The artifact integrates.","evidence_citations":[0]}}],"proportion":null}});
    let report: Report = serde_json::from_value(value.clone()).unwrap();
    assert!(
        decisions::verify(&request, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_REQUIREMENT_COVERAGE")
    );
    let mut complete = value;
    complete["decision_units"]["requirements"][0]["authority_items"] =
        json!(["goal", "private-notes"]);
    assert!(
        decisions::verify(&request, &serde_json::from_value(complete).unwrap())
            .unwrap()
            .is_some()
    );
}

#[test]
fn new_native_reports_reject_free_form_compound_facts_but_keep_old_reports_readable() {
    use kah_cli::observer_review::workspace;
    let request = Request {
        id: "compound".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let report: Report = serde_json::from_value(json!({"request_id":"compound","findings":[],"decision_units":{"requirements":[{"id":"integration","name":"Integration","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"fact","condition":"Composition is correct, HEAD is tagged, and three tests pass.","evidence_citations":[0]}}],"proportion":null}})).unwrap();
    assert!(
        workspace::require_units(&request, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_ATOMIC_FACT_REQUIRED")
    );
    assert_eq!(report.decision_units.unwrap().requirements.len(), 1);
}
