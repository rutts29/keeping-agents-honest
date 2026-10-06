use kah_cli::observer_review::{
    checkpoint_claim, questions,
    state::{Purpose, Request, Store},
};
use kah_observer::{
    judge::{Answer, Judgment},
    review_evidence::{Finding, Report, Topic},
};
use serde_json::json;

#[test]
fn one_rethink_then_one_named_question_parks_only_the_uncertain_request() {
    let worker = format!("claim-wait-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let (request, report, judgment) = inputs(&worker);
    assert!(
        checkpoint_claim::record_uncertainty(&request, &report, &judgment)
            .unwrap()
            .is_none()
    );
    // Re-sampling the same report isn't another thinker reconsideration.
    assert!(
        checkpoint_claim::record_uncertainty(&request, &report, &judgment)
            .unwrap()
            .is_none()
    );
    kah_cli::unattended_report::save(
        &store.directory,
        "redelivery.json",
        &json!({"requests":{&request.id:{"attempts":[{"status":"submitted_waiting_for_report"}]}}}),
    )
    .unwrap();
    let assessment = checkpoint_claim::record_uncertainty(&request, &report, &judgment)
        .unwrap()
        .unwrap();
    assert!(assessment["passed"].is_null());
    assert_eq!(assessment["state"], "checkpoint_needs_user");
    assert!(checkpoint_claim::waiting(&worker, &request.id).unwrap());
    assert!(!checkpoint_claim::waiting(&worker, "review-unrelated").unwrap());
    checkpoint_claim::record_uncertainty(&request, &report, &judgment)
        .unwrap()
        .unwrap();
    let pending = questions::pending(&store.directory).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["goal_at_question"], request.intent["goal"]);
    assert!(
        questions::message(&pending[0])
            .unwrap()
            .contains("Work remains active and unproven.")
    );
    std::fs::remove_dir_all(store.directory.parent().unwrap()).unwrap();
}

fn inputs(worker: &str) -> (Request, Report, Judgment) {
    let request = Request {
        id: "review-claim-wait".into(),
        worker: worker.to_owned(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: json!({"goal":"Complete the authorized work verbatim","revision":4,"items":[]}),
        trigger: json!({"events":[{"kind":"native_phase_claim","uncertain":true,"source":{"native_assistant_text":"Work remains active and unproven."}}]}),
        documents: vec![],
        created_ms: 0,
    };
    let report = Report {
        request_id: request.id.clone(),
        findings: vec![Finding {
            topic: Topic::Claim,
            claim: "The message describes ongoing work.".into(),
            citations: vec![],
            deferral: None,
            checkpoint_scope: None,
        }],
        reminder: None,
        decision_units: None,
    };
    let judgment = Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: [("claim_made".into(), Answer::Noul { noul: 0.76 })].into(),
        metrics: Default::default(),
        fallback_reason: None,
    };
    (request, report, judgment)
}
