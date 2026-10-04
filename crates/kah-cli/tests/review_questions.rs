use kah_cli::observer_review::{
    questions,
    state::{Purpose, Request},
};
use kah_observer::{decision_units::ReviewUnits, review_evidence::Report};
use serde_json::json;

fn request() -> Request {
    Request {
        id: "review-first".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("sha".into()),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    }
}
fn report() -> Report {
    let units:ReviewUnits=serde_json::from_value(json!({"requirements":[{"id":"format","name":"Output format","dimension":"intent","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"needs_user","reason":"ambiguous_instructions","summary":"Exact and normalized output are both plausible.","question":"Which output format should apply?","options":["Exact","Normalized"],"evidence_citations":[0],"ledger_review":{"reasoning":"No prior format decision resolves this.","citations":[0]}}}],"proportion":null})).unwrap();
    let mut r: Report =
        serde_json::from_value(json!({"request_id":"review-first","findings":[]})).unwrap();
    r.decision_units = Some(units);
    r
}
#[test]
fn named_question_is_durable_deduplicated_and_scoped_to_one_requirement() {
    let root = std::env::temp_dir().join(format!("kah-question-unit-{}", std::process::id()));
    let first = questions::store(
        &root,
        &request(),
        &report(),
        &json!({"needs_user":["format"],"needs_thinker":[]}),
    )
    .unwrap();
    assert_eq!(first.len(), 1);
    let mut later = request();
    later.id = "review-later".into();
    assert!(
        questions::store(&root, &later, &report(), &json!({"needs_user":["format"]}))
            .unwrap()
            .is_empty()
    );
    let pending = questions::pending(&root).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["requirement"], "format");
    assert!(
        questions::message(&pending[0])
            .unwrap()
            .contains("Unrelated authorized work can continue")
    );
    std::fs::remove_dir_all(root).unwrap();
}
