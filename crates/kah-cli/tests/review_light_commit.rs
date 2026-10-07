//! USER private-receipt-omitted: one light report, no rethink/verify-first.
use kah_cli::observer_review::{
    light_commit,
    state::{Purpose, Request, Store},
    uncertainty,
};
use kah_observer::{
    judge::{Judgment, Metrics},
    review_evidence::Report,
};
use serde_json::json;
#[test]
fn uncertain_light_report_names_builder_work_without_another_round_or_gate_question() {
    let worker = format!("light-commit-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let request = Request {
        id: "review-light".into(),
        worker,
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![],
        created_ms: 0,
    };
    let report: Report =
        serde_json::from_value(json!({"request_id":request.id,"findings":[]})).unwrap();
    let assessment = light_commit::assessment(
        &json!({"passed":null,"state":"checkpoint_needs_thinker","needs_thinker":["normalize"],"needs_user":[],"violations":[]}),
    );
    let judgment = Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    uncertainty::record(&store, &request, &report, &assessment, &judgment).unwrap();
    assert_eq!(assessment["state"], "commit_needs_check");
    assert_eq!(assessment["needs_check"], json!(["normalize"]));
    assert!(!store.directory.join("queued").exists());
    assert!(!store.directory.join("gate-followups").exists());
    assert!(
        kah_cli::observer_review::questions::pending(&store.directory)
            .unwrap()
            .is_empty()
    );
    let reminders = kah_cli::observer_review::reminder_queue::pending(&store.directory).unwrap();
    assert_eq!(reminders.len(), 1);
    assert!(
        reminders[0].1["text"]
            .as_str()
            .unwrap()
            .contains("normalize")
    );
    assert!(
        reminders[0].1["text"]
            .as_str()
            .unwrap()
            .contains("needs_check")
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn light_verdict_never_hides_a_violation_or_a_real_user_wait() {
    assert_eq!(
        light_commit::assessment(
            &json!({"violations":["privacy"],"needs_thinker":[],"needs_user":["scope"]})
        )["state"],
        "commit_failed"
    );
    assert_eq!(
        light_commit::assessment(
            &json!({"violations":[],"needs_thinker":[],"needs_user":["scope"]})
        )["state"],
        "commit_needs_user"
    );
    assert_eq!(
        light_commit::assessment(&json!({"violations":[],"needs_thinker":[],"needs_user":[]}))["passed"],
        true
    );
}
#[test]
fn supported_executable_without_current_sha_checks_still_needs_builder_evidence() {
    let request = Request {
        id: "review-light".into(),
        worker: "w".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Implement normalize"}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![],
        created_ms: 0,
    };
    let mut report:Report=serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"requirements":[{"id":"normalize","name":"normalize behavior","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Looks right.","evidence_citations":[0]}}]}})).unwrap();
    let mut assessment = json!({"passed":true,"violations":[],"needs_user":[],"needs_thinker":[],"not_due":[],"deferred":[],"resolved_non_executable":[],"resolved_preservation":[]});
    light_commit::coverage(&request, &report, &mut assessment);
    assert_eq!(assessment["needs_check"], json!(["normalize"]));
    assert_ne!(assessment["passed"], true);
    let check = kah_observer::decision_units::CheckReceipt {
        name: "behavior test".into(),
        command_citation: 0,
        result_citation: 1,
        sha_citation: 2,
        sha: "b".repeat(40),
    };
    report.decision_units.as_mut().unwrap().requirements[0]
        .checks
        .push(check);
    light_commit::coverage(&request, &report, &mut assessment);
    assert_ne!(
        assessment["passed"], true,
        "old SHA cannot establish current behavior"
    );
}
