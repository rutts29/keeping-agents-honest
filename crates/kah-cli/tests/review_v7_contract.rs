use kah_cli::observer_review::{
    completion_due,
    state::{Purpose, Request},
    workspace,
};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
fn request() -> Request {
    Request {
        id: "review-current".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({"goal":"Complete CSV phase."}),
        trigger: json!({"events":[{"kind":"user_gate","source":"host_cli"}]}),
        documents: vec![],
        created_ms: 0,
    }
}
fn raw() -> Value {
    json!({"request_id":"review-current","findings":[],"decision_units":{"version":7,"requirements":[{
        "id":"csv","name":"CSV phase integration","dimension":"integration","authority_items":["goal"],"blocker":null,"citations":[],"components":[],"checks":[],
        "applicability":{"due":true,"unconditional":false,"phase_completion":true,"reasoning":"This phase is at a gate.","evidence_citations":[0]},
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The CSV pipeline ran.","evidence_citations":[0]}
    }]}})
}
#[test]
fn current_native_reports_cannot_silently_use_the_old_protocol() {
    let mut r = raw();
    r["decision_units"]
        .as_object_mut()
        .unwrap()
        .remove("version");
    let report: Report = serde_json::from_value(r).unwrap();
    assert!(
        workspace::require_units(&request(), &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_PROTOCOL_REQUIRED")
    );
}
#[test]
fn a_current_phase_completion_requirement_must_use_the_supplied_controller_receipt() {
    let mut req = request();
    completion_due::add(&mut req).unwrap();
    let report: Report = serde_json::from_value(raw()).unwrap();
    assert!(
        workspace::require_units(&req, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_COMPLETION_RECEIPT_REQUIRED")
    );
    let mut r = raw();
    r["decision_units"]["requirements"][0]["applicability"]["phase_completion"] = json!(false);
    let report: Report = serde_json::from_value(r).unwrap();
    workspace::require_units(&req, &report).unwrap();
}
#[test]
fn native_contract_explains_typed_blockers_exact_questions_and_literal_timing() {
    for purpose in [Purpose::Drift, Purpose::Checkpoint, Purpose::Steering] {
        let s = workspace::decision_instructions(&purpose);
        assert!(s.contains("version=7"));
        assert!(s.contains("future_implementation"));
        assert!(s.contains("worker_question"));
        assert!(s.contains("phase_completion"));
        assert!(s.contains("controller_due"));
    }
}
