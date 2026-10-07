use kah_cli::observer_review::{
    capture::controller_receipts,
    decisions,
    state::{Purpose, Request},
};
use kah_observer::{
    decision_units::{Dimension, Thinking, controller_fact::Fact},
    review_evidence::{Document, Report},
    verification_contract::ReceiptFact,
};
use serde_json::json;
fn request() -> Request {
    let head = "a".repeat(40);
    Request {
        id: "review-host".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some(head.clone()),
        intent: json!({"goal":"Verify delivery"}),
        trigger: json!({"decision_protocol_version":11,"events":[{"kind":"run_completion","completion_key":{"commit":head},"verification":{"commit":head,"command":"node verify.cjs","exit_code":0,"success":true,"elapsed_ms":1,"output":"PASS\n","output_truncated":false}}]}),
        documents: vec![Document::new(
            "git",
            "controller_facts",
            "kah:git",
            &json!({"tags":format!("{head} refs/tags/gate-ready")}).to_string(),
        )],
        created_ms: 0,
    }
}
fn report(request: &Request) -> Report {
    let doc = request
        .documents
        .iter()
        .find(|d| d.id == "controller-receipts")
        .unwrap();
    let fact = Fact {
        worker: request.worker.clone(),
        request: request.id.clone(),
        head: request.head.clone().unwrap(),
        receipt_citation: 0,
        property: ReceiptFact::TestExit {
            test_index: 0,
            expected: 0,
        },
    };
    serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":11,"requirements":[{"id":"exit","name":fact.label(),"dimension":"integration","authority_items":["goal"],"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],"components":[],"checks":[],"judgment":{"kind":"controller_fact","fact":fact,"evidence_citations":[0]}}]}})).unwrap()
}
#[test]
fn host_capture_binds_controller_properties_without_worker_tool_provenance() {
    let mut r = request();
    controller_receipts::add(&mut r).unwrap();
    let report = report(&r);
    decisions::verify_evidence(&r, &report).unwrap();
    let questions =
        kah_observer::decision_units::questions(report.decision_units.as_ref().unwrap());
    assert!(questions.is_empty());
    let mut changed = r.clone();
    changed.worker = "foreign".into();
    assert!(decisions::verify_evidence(&changed, &report).is_err());
    changed = r.clone();
    changed.head = Some("b".repeat(40));
    assert!(decisions::verify_evidence(&changed, &report).is_err());
    let mut forged = r.clone();
    forged
        .documents
        .iter_mut()
        .find(|d| d.id == "controller-receipts")
        .unwrap()
        .kind = "worker_transcript".into();
    assert!(decisions::verify_evidence(&forged, &report).is_err());
}
#[test]
fn controller_facts_cannot_stand_in_for_checkpoint_semantics() {
    let mut r = request();
    controller_receipts::add(&mut r).unwrap();
    let mut report = report(&r);
    let units = report.decision_units.as_mut().unwrap();
    let base = units.requirements[0].clone();
    units.requirements = (0..3)
        .map(|i| {
            let mut x = base.clone();
            x.id = format!("fact{i}");
            x.dimension = [Dimension::Intent, Dimension::Integration, Dimension::Drift][i];
            x
        })
        .collect();
    assert!(decisions::checkpoint_coverage(&r, units, &[]).is_err());
    assert!(matches!(
        units.requirements[0].judgment,
        Thinking::ControllerFact { .. }
    ));
}
