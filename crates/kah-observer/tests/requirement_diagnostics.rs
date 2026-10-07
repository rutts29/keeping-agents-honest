use kah_observer::{decision_units::ReviewUnits, review_evidence::Document};
use serde_json::json;
#[test]
fn empty_requirement_names_every_missing_field_and_the_requirement_id() {
    let units: ReviewUnits = serde_json::from_value(json!({"requirements":[{"id":"normalize","name":" ","dimension":"intent","authority_items":[],"citations":[],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"reason","evidence_citations":[]}}]})).unwrap();
    let error = units.verify(&[], &["goal".into()]).unwrap_err();
    for text in [
        "REVIEW_REQUIREMENT_EMPTY",
        "normalize",
        "name",
        "authority_items",
        "citations",
    ] {
        assert!(error.contains(text), "{error}");
    }
}
#[test]
fn nested_thinking_and_receipt_errors_keep_the_requirement_identity_and_field() {
    let doc = Document::new("ledger", "ledger", "kah:ledger", "Whole intent.");
    let mut units: ReviewUnits = serde_json::from_value(json!({"requirements":[{"id":"normalize","name":"normalize works","dimension":"intent","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":" ","evidence_citations":[0]}}]})).unwrap();
    let error = units
        .verify(std::slice::from_ref(&doc), &["goal".into()])
        .unwrap_err();
    assert!(
        error.contains("normalize") && error.contains("judgment.reasoning"),
        "{error}"
    );
    units.requirements[0].judgment = serde_json::from_value(json!({"kind":"semantic","decision":"satisfied","reasoning":"Whole reason.","evidence_citations":[5]})).unwrap();
    let error = units.verify(&[doc], &["goal".into()]).unwrap_err();
    assert!(
        error.contains("normalize") && error.contains("evidence_citations"),
        "{error}"
    );
}
#[test]
fn empty_ledger_does_not_require_fabricated_authority_but_live_intent_does() {
    let doc = Document::new(
        "transcript",
        "worker_transcript",
        "native:fixture",
        "Observed native work.",
    );
    let units: ReviewUnits = serde_json::from_value(json!({"requirements":[{"id":"observed","name":"Observed native work","dimension":"intent","authority_items":[],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Observed evidence only; no user intent exists.","evidence_citations":[0]}}]})).unwrap();
    assert!(units.verify(std::slice::from_ref(&doc), &[]).is_ok());
    let error = units.verify(&[doc], &["goal".into()]).unwrap_err();
    assert!(
        error.contains("observed") && error.contains("authority_items"),
        "{error}"
    );
}
