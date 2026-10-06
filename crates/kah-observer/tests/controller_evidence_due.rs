use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Judgment, Metrics},
    review_evidence::Document,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(occurred: bool) -> (ReviewUnits, Document) {
    let receipt = json!({"worker":"worker","request":"review-current","head":"a".repeat(40),"event":"observer_evidence_ask","occurred":occurred,"receipts":[]});
    let doc = Document::new(
        "controller-evidence",
        "controller_event",
        "kah:controller-evidence",
        &receipt.to_string(),
    );
    let units = serde_json::from_value(json!({"version":9,"requirements":[{
        "id":"response","name":"Answer the observer's evidence request","dimension":"intent","authority_items":["goal"],"blocker":null,
        "citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],
        "applicability":{"due":occurred,"unconditional":false,"reasoning":"The host records whether it sent an evidence request.","evidence_citations":[0],"controller_trigger":{"receipt_citation":0,"receipt":receipt}},
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The response is independently checked when due.","evidence_citations":[0]}
    }]})).unwrap();
    (units, doc)
}
fn judgment() -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::new(),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn no_controller_ask_is_literal_not_due_without_any_jev_answer() {
    let (units, doc) = fixture(false);
    units.verify(&[doc], &["goal".into()]).unwrap();
    assert!(decision_units::questions(&units).is_empty());
    let outcome = decision_units::assess(&units, &judgment());
    assert!(outcome.passed);
    assert_eq!(outcome.not_due, ["response"]);
}
#[test]
fn an_accepted_ask_establishes_due_but_does_not_prove_the_response() {
    let (units, doc) = fixture(true);
    units.verify(&[doc], &["goal".into()]).unwrap();
    let questions = decision_units::questions(&units);
    assert!(!questions.contains_key("applicability.response"));
    assert!(questions.contains_key("requirement.response"));
    assert_eq!(
        decision_units::assess(&units, &judgment()).needs_thinker,
        ["response"]
    );
}
#[test]
fn binding_forgery_and_unconditional_override_are_rejected() {
    for field in ["worker", "request", "head"] {
        let (units, doc) = fixture(false);
        let mut raw = serde_json::to_value(units).unwrap();
        raw["requirements"][0]["applicability"]["controller_trigger"]["receipt"][field] =
            json!("foreign");
        let units: ReviewUnits = serde_json::from_value(raw).unwrap();
        assert!(units.verify(&[doc], &["goal".into()]).is_err());
    }
    let (units, mut doc) = fixture(false);
    doc.kind = "worker_transcript".into();
    assert!(units.verify(&[doc], &["goal".into()]).is_err());
    let (units, doc) = fixture(true);
    let mut raw: Value = serde_json::to_value(units).unwrap();
    raw["requirements"][0]["applicability"]["unconditional"] = json!(true);
    let units: ReviewUnits = serde_json::from_value(raw).unwrap();
    assert!(units.verify(&[doc], &["goal".into()]).is_err());
}
