use kah_observer::{
    decision_units::{Requirement, controller_fact::Fact},
    review_evidence::Document,
};
use serde_json::{Value, json};
fn fixture(property: Value) -> (Fact, Requirement, Vec<Document>) {
    let head = "a".repeat(40);
    let host = json!({"worker":"worker","request":"review-fact","head":head,"fresh_checkout":true,"tests":[{"commit":head,"command":"node verify.cjs","exit_code":0,"output":"PASS\n"}],"tags":[{"name":"gate-ready","head":head}]});
    let doc = Document::new(
        "controller-receipts",
        "controller_verification",
        "kah:controller-receipts",
        &host.to_string(),
    );
    let fact:Fact=serde_json::from_value(json!({"worker":"worker","request":"review-fact","head":head,"receipt_citation":0,"property":property})).unwrap();
    let requirement:Requirement=serde_json::from_value(json!({"id":"fact","name":fact.label(),"dimension":"integration","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Fixture","evidence_citations":[0]}})).unwrap();
    (fact, requirement, vec![doc])
}
#[test]
fn literal_host_properties_compare_in_code() {
    for property in [
        json!({"property":"head_sha"}),
        json!({"property":"test_exit","test_index":0,"expected":0}),
        json!({"property":"test_output_line","test_index":0,"expected":"PASS"}),
        json!({"property":"tag_head","tag":"gate-ready"}),
    ] {
        let (fact, r, docs) = fixture(property);
        fact.verify(&r, &docs).unwrap();
    }
}
#[test]
fn worker_sources_or_behavior_claims_cannot_be_controller_facts() {
    let (fact, mut r, mut docs) = fixture(json!({"property":"head_sha"}));
    r.name = "Normalize implements rounding".into();
    assert!(fact.verify(&r, &docs).is_err());
    r.name = fact.label();
    docs[0].kind = "worker_transcript".into();
    assert!(fact.verify(&r, &docs).is_err());
}
#[test]
fn bindings_and_contradictory_properties_are_rejected() {
    let (mut fact, r, docs) = fixture(json!({"property":"head_sha"}));
    fact.worker = "other".into();
    assert!(fact.verify(&r, &docs).is_err());
    let (fact, r, docs) = fixture(json!({"property":"test_exit","test_index":0,"expected":1}));
    assert!(fact.verify(&r, &docs).is_err());
    let (fact, r, docs) =
        fixture(json!({"property":"test_output_line","test_index":0,"expected":"ABSENT"}));
    assert!(fact.verify(&r, &docs).is_err());
}

#[test]
fn typed_controller_fact_is_verified_without_jev_questions() {
    let (fact, mut r, docs) = fixture(json!({"property":"test_exit","test_index":0,"expected":0}));
    r.judgment = serde_json::from_value(
        json!({"kind":"controller_fact","fact":fact,"evidence_citations":[0]}),
    )
    .unwrap();
    let units = kah_observer::decision_units::ReviewUnits {
        version: Some(11),
        requirements: vec![r],
        proportion: None,
        steering: None,
    };
    units.verify(&docs, &["goal".into()]).unwrap();
    assert!(kah_observer::decision_units::questions(&units).is_empty());
}
