//! Approved commit-cache-v1; exercise the ordinary authority verifier.
use kah_cli::observer_review::{
    decisions,
    state::{Purpose, Request},
    verdict_cache::integration,
};
use kah_observer::{
    judge::{Judgment, Metrics},
    review_evidence::{Document, Report},
};
use serde_json::json;
fn fixture() -> (Request, Report) {
    let ledger = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "Goal words.\nCached user words.\n",
    );
    let mut request = Request {
        id: "review-cache-next".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("b".repeat(40)),
        intent: json!({"goal":"Goal words.","items":[{"id":"user-cached","text":"Cached user words."}]}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![ledger.clone()],
        created_ms: 1,
    };
    let requirement = json!({"id":"cached","name":"Cached requirement","dimension":"integration","authority_items":["user-cached"],
        "citations":[{"source":"ledger","digest":ledger.digest,"quote":"Cached user words."}],"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Exact unchanged evidence","evidence_citations":[0]}});
    let cache = Document::new("cached-verdicts","controller_cached_verdicts","kah:checked-cache",
        &json!({"protocol":"commit-cache-v1","worker":request.worker,"request":request.id,"carried":[{
            "requirement":"cached","carried_from_review":"review-old","carried_from_sha":"a".repeat(40),
            "verdict":{"requirement":requirement,"verified":{"id":"cached"},"answers":{"requirement.cached":{
                "type":"choice","choice":"supported","confidence":0.99,"probabilities":{"supported":0.99,"contradicted":0.005,"unclear":0.005}}}}
        }]}).to_string());
    request.trigger["cache_plan"] =
        json!({"protocol":"commit-cache-v1","record_digest":cache.digest});
    request.documents.push(cache);
    let report = serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":12,
        "requirements":[{"id":"fresh","name":"Fresh intent overview","dimension":"intent","authority_items":["goal"],
            "citations":[{"source":"ledger","digest":ledger.digest,"quote":"Goal words."}],"components":[],"checks":[],
            "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Current evidence","evidence_citations":[0]}}]}})).unwrap();
    (request, report)
}
#[test]
fn fresh_and_carried_cover_authority_but_a_heavy_gate_still_requires_all_items_fresh() {
    let (mut request, report) = fixture();
    decisions::verify_evidence(&request, &report).unwrap();
    assert_eq!(
        integration::units(&request, &report)
            .unwrap()
            .requirements
            .len(),
        2
    );
    request.trigger["review_class"] = "gate".into();
    assert!(
        decisions::verify_evidence(&request, &report)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_REQUIREMENT_COVERAGE")
    );
}
#[test]
fn tampered_digest_rejects_and_a_fresh_same_id_replaces_the_carried_unit() {
    let (mut request, mut report) = fixture();
    request.trigger["cache_plan"]["record_digest"] = "wrong".into();
    assert!(
        decisions::verify_evidence(&request, &report)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_CACHE_BINDING")
    );
    let (request, _) = fixture();
    let mut replacement = integration::units(&request, &report)
        .unwrap()
        .requirements
        .remove(1);
    replacement.name = "New evidence needs fresh judgment".into();
    report
        .decision_units
        .as_mut()
        .unwrap()
        .requirements
        .push(replacement);
    assert!(integration::carried(&request, &report).unwrap().is_empty());
    assert_eq!(
        integration::units(&request, &report)
            .unwrap()
            .requirements
            .len(),
        2
    );
}
#[test]
fn answers_are_carried_without_new_usage_and_lineage_is_visible() {
    let (request, report) = fixture();
    let mut judgment = Judgment {
        backend: "jev".into(),
        model: "jev".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    let mut assessment = json!({});
    integration::merge(&request, &report, &mut judgment, &mut assessment).unwrap();
    assert!(judgment.answers.contains_key("requirement.cached"));
    assert_eq!(judgment.metrics.calls, 0);
    assert_eq!(assessment["carried"][0]["review"], "review-old");
    assert_eq!(assessment["carried"][0]["head"], "a".repeat(40));
}
