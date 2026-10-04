use kah_cli::observer_review::{
    deferrals,
    state::{Purpose, Request},
};
use serde_json::{Value, json};
fn request() -> Request {
    Request {
        id: "review-original".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("sha".into()),
        intent: json!({"open_deferrals":[]}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    }
}
fn finding(claim: &str, id: Option<&str>, phase: &str) -> Value {
    json!({"evidence":{"topic":"deferral","model_claim_not_a_decision":claim,"deferral":{"id":id,"phase":phase,"requirement_citation":0,"dependency_citation":1},"citations":[{"uri":"docs/PLAN.md","quote":"Wire the external service into the command."},{"uri":"docs/PLAN.md","quote":"Phase 3 provides the external service."}]}})
}
#[test]
fn deferral_identity_survives_paraphrasing_and_cannot_move_to_a_later_phase() {
    let mut request = request();
    let first = deferrals::collect(
        &request,
        &[finding("Blocked by external service", None, "Phase 3")],
    )
    .unwrap();
    let second = deferrals::collect(
        &request,
        &[finding(
            "The service adapter isn't here yet",
            None,
            "Phase 3",
        )],
    )
    .unwrap();
    assert_eq!(first[0]["id"], second[0]["id"]);
    request.id = "review-later".into();
    request.intent["open_deferrals"] = json!(first);
    let id = request.intent["open_deferrals"][0]["id"].as_str().unwrap();
    let updated = deferrals::collect(
        &request,
        &[finding("Revisited dependency", Some(id), "Phase 3")],
    )
    .unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0]["introduced_at"], "review-original");
    assert!(
        deferrals::collect(&request, &[finding("Postpone again", Some(id), "Phase 4")])
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_DEFERRAL_REDEFINED")
    );
    assert!(
        deferrals::collect(
            &request,
            &[finding("Made up identity", Some("new-model-id"), "Phase 3")]
        )
        .is_err()
    );
}
