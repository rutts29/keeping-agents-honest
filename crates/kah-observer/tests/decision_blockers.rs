use kah_observer::{
    decision_units::ReviewUnits,
    review_evidence::{Document, NativeRecord},
};
use serde_json::{Value, json};

fn units(doc: &Document, blocker: Value, judgment: Value) -> ReviewUnits {
    serde_json::from_value(json!({"version":7,"requirements":[{
        "id":"policy","name":"CSV quoting policy","dimension":"intent","authority_items":["goal"],
        "blocker":blocker,"citations":[{"source":doc.id,"digest":doc.digest,"quote":doc.text}],
        "components":[],"checks":[],"judgment":judgment
    }]}))
    .unwrap()
}
fn deferred() -> Value {
    json!({"kind":"semantic","decision":"deferred","reasoning":"The renderer is explicitly assigned to a later phase.","evidence_citations":[0]})
}
fn question() -> Value {
    json!({"kind":"needs_user","reason":"ambiguous_instructions","summary":"No user policy decision exists.","question":"Preserve CSV quoting or normalize it?","options":["Preserve","Normalize"],"evidence_citations":[0],"ledger_review":{"reasoning":"The ledger contains no answer.","citations":[0]}})
}
#[test]
fn user_decision_blocker_cannot_hide_as_a_semantic_deferral() {
    let doc = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "The user has not chosen CSV quoting.",
    );
    let error = units(&doc, json!("user_decision"), deferred())
        .verify(&[doc], &["goal".into()])
        .unwrap_err();
    assert!(error.starts_with("REVIEW_BLOCKER_USER_QUESTION"));
}
#[test]
fn future_implementation_blocker_retains_the_reasoned_deferral() {
    let doc = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "Build the CSV renderer in Phase 2.",
    );
    let u = units(&doc, json!("future_implementation"), deferred());
    assert_eq!(u.version(), 7);
    assert_eq!(
        u.verify(&[doc], &["goal".into()]).unwrap()["requirements"][0]["blocker"],
        "future_implementation"
    );
}
#[test]
fn needs_user_blocker_preserves_the_ledger_check() {
    let doc = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "No CSV policy decision exists.",
    );
    assert!(
        units(&doc, json!("user_decision"), question())
            .verify(&[doc], &["goal".into()])
            .is_ok()
    );
}
#[test]
fn the_workers_question_is_exact_and_requires_native_assistant_provenance() {
    let ledger = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "No CSV policy decision exists.",
    );
    let mut native = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "Preserve CSV quoting or normalize it?",
    );
    native.native_records = vec![NativeRecord {
        start_byte: 0,
        end_byte: native.text.len(),
        context: json!({"record_type":"agentMessage"}),
    }];
    let mut u = units(&ledger, json!("user_decision"), question());
    let mut raw = serde_json::to_value(&u).unwrap();
    raw["requirements"][0]["worker_question"] = json!(1);
    raw["requirements"][0]["citations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source":native.id,"digest":native.digest,"quote":native.text}));
    u = serde_json::from_value(raw).unwrap();
    assert!(
        u.verify(&[ledger.clone(), native.clone()], &["goal".into()])
            .is_ok()
    );
    native.native_records[0].context =
        json!({"record_type":"assistant","message_role":"assistant","content_types":["text"]});
    assert!(
        u.verify(&[ledger.clone(), native.clone()], &["goal".into()])
            .is_ok()
    );
    for context in [
        json!({"record_type":"userMessage"}),
        json!({"record_type":"agentMessage","is_meta":true}),
        json!({"record_type":"agentMessage","is_sidechain":true}),
        json!({"record_type":"assistant","message_role":"assistant","content_types":["tool_use"]}),
    ] {
        native.native_records[0].context = context;
        assert!(
            u.verify(&[ledger.clone(), native.clone()], &["goal".into()])
                .unwrap_err()
                .starts_with("REVIEW_WORKER_QUESTION_PROVENANCE")
        );
    }
    let mut raw = serde_json::to_value(u).unwrap();
    raw["requirements"][0]["judgment"]["question"] = json!("Which CSV policy would you prefer?");
    let u: ReviewUnits = serde_json::from_value(raw).unwrap();
    native.native_records[0].context = json!({"record_type":"agentMessage"});
    assert!(
        u.verify(&[ledger, native], &["goal".into()])
            .unwrap_err()
            .starts_with("REVIEW_WORKER_QUESTION_EXACT")
    );
}
