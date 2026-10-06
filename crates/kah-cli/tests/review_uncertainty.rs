use kah_cli::observer_review::{
    state::{Purpose, Request},
    uncertainty,
};
use kah_observer::review_evidence::Report;
use serde_json::json;

#[test]
fn uncertain_notice_names_the_requirement_and_preserves_the_full_record() {
    let request = Request {
        id: "review-uncertain".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let report: Report = serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"requirements":[{"id":"format","name":"Output format","dimension":"intent","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Evidence needs further interpretation.","evidence_citations":[0]}}],"proportion":null}})).unwrap();
    let note = uncertainty::notice(
        &request,
        &report,
        &json!({"needs_thinker":["format"],"needs_user":[]}),
    )
    .unwrap();
    assert!(note.contains("Output format (format)"));
    assert!(note.contains("review-uncertain"));
    assert!(note.contains("Unrelated authorized work can continue"));
    assert!(
        uncertainty::notice(
            &request,
            &report,
            &json!({"needs_thinker":[],"needs_user":[]})
        )
        .is_none()
    );
}

#[test]
fn drift_reconsideration_survives_until_an_event_and_keeps_its_original_decision() {
    let root = std::env::temp_dir().join(format!("kah-rethink-unit-{}", std::process::id()));
    let request = Request {
        id: "review-uncertain".into(),
        worker: "worker".into(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: Some("sha".into()),
        intent: json!({}),
        trigger: json!({"events":[{"kind":"phase_done"}]}),
        documents: vec![],
        created_ms: 0,
    };
    uncertainty::retain_rethink(
        &root,
        &request,
        &json!({"needs_thinker":["integration"],"needs_user":[]}),
    )
    .unwrap();
    let wake = uncertainty::rethink(&root).unwrap().unwrap();
    assert_eq!(wake.purpose, Purpose::Drift);
    assert_eq!(
        wake.trigger["reconsideration"]["request"],
        "review-uncertain"
    );
    assert_eq!(wake.trigger["events"][0]["kind"], "phase_done");
    uncertainty::retain_rethink(
        &root,
        &request,
        &json!({"needs_thinker":[],"needs_user":[]}),
    )
    .unwrap();
    assert!(uncertainty::rethink(&root).unwrap().is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_rethought_checkpoint_keeps_uncertainty_visible_without_another_reask() {
    // Allow one follow-up verification round.
    let root = std::env::temp_dir().join(format!("kah-no-reask-{}", std::process::id()));
    let request = Request {
        id: "review-followup".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({}),
        trigger: json!({"reconsideration":{"request":"review-first"}}),
        documents: vec![],
        created_ms: 0,
    };
    uncertainty::retain_rethink(&root, &request, &json!({"needs_thinker":["intent"]})).unwrap();
    assert!(uncertainty::rethink(&root).unwrap().is_none());
    assert!(!root.exists());
}

#[test]
fn checkpoint_rethink_is_durably_queued_once_without_another_worker_event() {
    let root = std::env::temp_dir().join(format!("kah-checkpoint-rethink-{}", std::process::id()));
    let request = Request {
        id: "review-original".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({}),
        trigger: json!({"events":[{"kind":"run_completion"}]}),
        documents: vec![],
        created_ms: 0,
    };
    let assessment = json!({"needs_thinker":["intent"],"needs_user":[]});
    uncertainty::retain_rethink(&root, &request, &assessment).unwrap();
    uncertainty::retain_rethink(&root, &request, &assessment).unwrap();
    let files = std::fs::read_dir(root.join("queued"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(files.len(), 1);
    let wake: kah_cli::observer_review::queue::Wake =
        kah_cli::observer_review::source_store::read(&files[0].path()).unwrap();
    assert_eq!(wake.purpose, Purpose::Checkpoint);
    assert_eq!(wake.trigger["reconsideration"]["request"], request.id);
    assert_eq!(wake.trigger["events"][0]["kind"], "run_completion");
    assert!(uncertainty::rethink(&root).unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
}
