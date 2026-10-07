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
    json!({"request_id":"review-current","findings":[],"decision_units":{"version":8,"requirements":[{
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
        // V10 contract keeps retained protocols and derives fresh submissions
        // from the captured request/schema, not a stale v8/v9 instruction.
        assert!(s.contains("exact const in this request's schema.json"));
        assert!(s.contains("New captured requests use v11"));
        assert!(s.contains("retained earlier requests keep their recorded protocol"));
        assert!(s.contains("judgment.kind=controller_fact"));
        assert!(s.contains("future_implementation"));
        assert!(s.contains("worker_question"));
        assert!(s.contains("phase_completion"));
        assert!(s.contains("controller_due"));
        assert!(s.contains("existing_question_id"));
        assert!(s.contains("own checked source manifest"));
    }
}

#[test]
fn controller_event_requests_require_v9_and_retained_requests_stay_v8() {
    let mut req = request();
    let report: Report = serde_json::from_value(raw()).unwrap();
    workspace::require_units(&req, &report).unwrap();
    req.documents
        .push(kah_observer::review_evidence::Document::new(
            "controller-evidence",
            "controller_event",
            "kah:controller-evidence",
            "{}",
        ));
    assert!(
        workspace::require_units(&req, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_PROTOCOL_REQUIRED")
    );
    let mut value = raw();
    value["decision_units"]["version"] = json!(9);
    let report: Report = serde_json::from_value(value).unwrap();
    workspace::require_units(&req, &report).unwrap();
}

#[test]
fn a_trigger_cannot_reuse_a_genuine_receipt_from_another_request() {
    let req = request();
    let mut value = raw();
    value["decision_units"]["version"] = json!(9);
    value["decision_units"]["requirements"][0]["applicability"]["controller_trigger"] = json!({"receipt_citation":0,"receipt":{"worker":"worker","request":"review-other","head":null,"event":"observer_evidence_ask","occurred":true,"receipts":[]}});
    let report: Report = serde_json::from_value(value).unwrap();
    assert!(
        kah_cli::observer_review::decisions::verify_evidence(&req, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_TRIGGER_BINDING")
    );
}
