use kah_cli::observer_review::{
    completion_due,
    state::{Purpose, Request},
};
use serde_json::json;
fn request() -> Request {
    Request {
        id: "review-current".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
        intent: json!({"goal":"Complete the CSV phase."}),
        trigger: json!({"events":[
            {"kind":"user_gate","source":"host_cli","at_ms":1},
            {"kind":"native_git_tag","source":"controller Git metadata","tags":["refs/tags/csv-1 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]},
            {"kind":"native_phase_claim","source":{"native_assistant_text":"CSV phase complete."},"judgment":{"backend":"jev","answers":{"phase_claim":{"type":"noul","noul":0.99}}},"uncertain":false}
        ]}),
        documents: vec![],
        created_ms: 0,
    }
}
#[test]
fn host_events_emit_bound_literal_phase_receipts_but_worker_marker_text_does_not() {
    let mut r = request();
    completion_due::add(&mut r).unwrap();
    let d = &r.documents[0];
    assert_eq!(d.kind, "controller_completion_due");
    let value: serde_json::Value = serde_json::from_str(&d.text).unwrap();
    assert_eq!(value["receipts"].as_array().unwrap().len(), 3);
    assert_eq!(value["worker"], r.worker);
    assert_eq!(value["request"], r.id);
    assert_eq!(value["head"], r.head.unwrap());
    let mut r = request();
    r.trigger = json!({"events":[{"kind":"user_gate","source":"worker marker"},{"kind":"native_git_tag","source":"repo file"},{"kind":"native_phase_claim","judgment":{"backend":"model-assertion"}}]});
    completion_due::add(&mut r).unwrap();
    assert!(r.documents.is_empty());
}
#[test]
fn midturn_and_uncertain_claims_do_not_become_unconditional_completion_events() {
    let mut r = request();
    r.purpose = Purpose::Drift;
    completion_due::add(&mut r).unwrap();
    assert!(r.documents.is_empty());
    let mut r = request();
    r.trigger = json!({"events":[{"kind":"native_phase_claim","judgment":{"backend":"jev"},"uncertain":true}]});
    completion_due::add(&mut r).unwrap();
    // Reviewer correction: candidate routing is not a confirmed completion fact.
    assert!(r.documents.is_empty());
}

#[test]
fn an_unconfirmed_point_76_phase_claim_remains_semantic_not_literal_due() {
    let mut r = request();
    r.trigger = json!({"events":[{"kind":"native_phase_claim","source":{"native_assistant_text":"CSV parser phase ready."},"judgment":{"backend":"jev","answers":{"phase_claim":{"type":"noul","noul":0.76}}},"uncertain":true}]});
    completion_due::add(&mut r).unwrap();
    assert!(
        r.documents.is_empty(),
        "a .76 interpretation is not a controller fact"
    );
}
