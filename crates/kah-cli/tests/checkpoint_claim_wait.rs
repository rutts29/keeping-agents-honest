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
fn one_rethink_then_visible_observation_never_asks_user_for_status() {
    let worker = format!("claim-wait-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let (request, mut report, judgment) = inputs(&worker);
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
    assert!(
        checkpoint_claim::record_uncertainty(&request, &report, &judgment)
            .unwrap()
            .is_none(),
        "transport retry is not a thinker reconsideration"
    );
    kah_cli::unattended_report::save(&store.directory, "redelivery.json", &json!({"requests":{&request.id:{"attempts":[{"status":"submitted_waiting_for_report","prior_error":"REVIEW_PHASE_CLAIM_UNCERTAIN stage=checkpoint"}]}}})).unwrap();
    report.findings[0].claim =
        "Reconsidered: the native message still describes ongoing work.".into();
    let assessment = checkpoint_claim::record_uncertainty(&request, &report, &judgment)
        .unwrap()
        .unwrap();
    assert!(assessment["passed"].is_null());
    assert_eq!(assessment["state"], "checkpoint_unconfirmed_observation");
    assert!(checkpoint_claim::waiting(&worker, &request.id).unwrap());
    assert!(!checkpoint_claim::waiting(&worker, "review-unrelated").unwrap());
    checkpoint_claim::record_uncertainty(&request, &report, &judgment)
        .unwrap()
        .unwrap();
    let pending = questions::pending(&store.directory).unwrap();
    assert_eq!(pending.len(), 0, "status is not a user decision");
    assert!(assessment["needs_user"].as_array().unwrap().is_empty());
    assert!(store.directory.join("claim-observations").is_dir());
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
        verification_risks: None,
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
