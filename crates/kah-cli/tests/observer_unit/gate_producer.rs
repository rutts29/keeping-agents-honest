use super::*;
fn proof() -> DeliveryProof {
    serde_json::from_value(json!({"head":"a".repeat(40),"fresh_checkout":true,
        "tests":[{"command":"check","commit":"a".repeat(40),"exit_code":0,"success":true}],
        "artifacts":[{"path":"PLAN.md","baseline_blob":"b".repeat(40),"delivered_blob":"b".repeat(40),"delivered_sha":"a".repeat(40)}]})).unwrap()
}
fn report() -> Report {
    serde_json::from_value(json!({"request_id":"review-final","findings":[],"decision_units":{"version":8,"requirements":[
        {"id":"intent","name":"Intent","dimension":"intent","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Evidence supports intent.","evidence_citations":[0]}},
        {"id":"integration","name":"Integration","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Evidence supports integration.","evidence_citations":[0]}}
    ]},"verification_risks":[{"requirement_id":"intent","unverified":"Intent interpretation still lacks independent confirmation.","acceptance_risk":"Acceptance may advance despite an unmet detail."}]})).unwrap()
}
fn pointer(receipt: &str) -> EvidencePointer {
    EvidencePointer {
        receipt: receipt.into(),
        record: format!("private:{receipt}"),
        digest: "d".repeat(64),
    }
}
fn round() -> VerificationRound {
    VerificationRound {
        id: "followup-123456789abcdef0123456789abcdef0".into(),
        binding: Binding {
            worker: "worker".into(),
            review: "review-first".into(),
            head: "a".repeat(40),
            intent_revision: 1,
        },
        stage: Stage::Checks,
        claimed: true,
        evidence: Some(vec![pointer("source")]),
        checks: None,
        builder: Some(BuilderReceipt {
            request: pointer("builder"),
            response: BuilderOutcome::Evidence {
                response: pointer("reply"),
            },
        }),
    }
}
fn job() -> Job {
    Job {
        request: "review-rethought".into(),
        proof: proof(),
        assessment: json!({"needs_thinker":["intent","integration"]}),
        report: report(),
    }
}
#[test]
fn verified_items_leave_risk_set_with_their_decision_receipts() {
    let (verified, risks) = items(
        &round(),
        &job(),
        &report(),
        &json!({"needs_thinker":["intent"]}),
        &pointer("checks"),
        &pointer("decision"),
    )
    .unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].requirement_id, "integration");
    assert_eq!(verified[0].decision_receipt.receipt, "decision");
    assert_eq!(risks.len(), 1);
    assert_eq!(risks[0].requirement_id, "intent");
}
#[test]
fn all_verified_items_need_no_user_risk_question() {
    let (verified, risks) = items(
        &round(),
        &job(),
        &report(),
        &json!({"needs_thinker":[]}),
        &pointer("checks"),
        &pointer("decision"),
    )
    .unwrap();
    assert_eq!(verified.len(), 2);
    assert!(risks.is_empty());
}
#[test]
fn renamed_items_cannot_disappear_as_verified() {
    let mut changed = report();
    changed.decision_units.as_mut().unwrap().requirements.pop();
    assert!(
        items(
            &round(),
            &job(),
            &changed,
            &json!({"needs_thinker":[]}),
            &pointer("checks"),
            &pointer("decision")
        )
        .is_err()
    );
}
#[test]
fn unknown_remaining_risk_cannot_be_omitted_from_question() {
    let mut changed = report();
    changed.verification_risks = None;
    assert!(
        items(
            &round(),
            &job(),
            &changed,
            &json!({"needs_thinker":["intent"]}),
            &pointer("checks"),
            &pointer("decision")
        )
        .is_err()
    );
}

#[test]
fn failed_controller_followup_checks_block_even_when_the_thinker_reports_pass() {
    let root = std::env::temp_dir().join(format!("kah-followup-guard-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("requests")).unwrap();
    let request = Request {
        id: "review-final".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({"verification_followup":{"stage":"checks_complete"}}),
        documents: vec![],
        created_ms: 0,
    };
    source_store::save(
        &root.join("requests"),
        "review-final.json",
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    let saved = start(&root, &request).unwrap();
    let path = directory(&root, &saved);
    kah_core::run_store::ensure_private_directory(&path).unwrap();
    let mut failed = proof();
    failed.tests[0].exit_code = Some(1);
    failed.tests[0].success = false;
    source_store::save(
        &path,
        "checks.json",
        &serde_json::to_value(&failed).unwrap(),
    )
    .unwrap();
    let mut assessment = json!({"passed":true,"violations":[]});
    guard(&root, &request, &mut assessment).unwrap();
    assert_eq!(assessment["passed"], false);
    assert_eq!(assessment["violations"], json!(["exact_sha_tests"]));
    guard(&root, &request, &mut assessment).unwrap();
    assert_eq!(assessment["violations"], json!(["exact_sha_tests"]));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn once_only_followup_queue_receipt_survives_transfer_without_duplicate_review() {
    let root = std::env::temp_dir().join(format!("kah-followup-queue-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    let wake = super::super::super::super::queue::Wake {
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        trigger: json!({"proof":"whole proof"}),
    };
    assert!(
        super::super::super::super::queue::enqueue_once_at(&root, "followup-checks", &wake)
            .unwrap()
    );
    let entries = fs::read_dir(root.join("queued"))
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let path = entries[0].as_ref().unwrap().path();
    kah_core::run_store::ensure_private_directory(&root.join("covered-triggers")).unwrap();
    fs::rename(
        &path,
        root.join("covered-triggers")
            .join(path.file_name().unwrap()),
    )
    .unwrap();
    assert!(
        !super::super::super::super::queue::enqueue_once_at(&root, "followup-checks", &wake)
            .unwrap()
    );
    assert!(fs::read_dir(root.join("queued")).unwrap().next().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_semantic_requirement_cannot_be_marked_verified_by_relabeling_it_as_a_fact() {
    let mut changed = serde_json::to_value(report()).unwrap();
    changed["decision_units"]["requirements"][0]["judgment"] = json!({"kind":"atomic_fact","fact":{"property":"exit_status","check":0,"expected":0},"evidence_citations":[0]});
    let changed: Report = serde_json::from_value(changed).unwrap();
    assert!(
        items(
            &round(),
            &job(),
            &changed,
            &json!({"needs_thinker":[]}),
            &pointer("checks"),
            &pointer("decision")
        )
        .is_err()
    );
}

#[test]
fn all_verified_round_commits_passing_checkpoint_clears_pending_and_creates_no_question() {
    use crate::observer_review::{
        checkpoint, consume, questions,
        state::{Pending, State},
    };
    let (root, store, request, path) = completed_fixture();
    let assessment = json!({"state":"checkpoint_passed","passed":true,"violations":[],"needs_user":[],"needs_thinker":[],"unproven":[]});
    let result =
        json!({"request":request.id,"head":request.head,"report":report(),"assessment":assessment});
    kah_core::run_store::ensure_private_directory(&root.join("results")).unwrap();
    source_store::save(&root.join("results"), "review-final.json", &result).unwrap();
    assert!(finish(&root, &request, &report(), &assessment).unwrap());
    let complete = completed(&root, &request).unwrap().unwrap();
    assert_eq!(complete.round().stage, Stage::Complete);
    assert_eq!(complete.round().checks.as_ref().unwrap().verified.len(), 2);
    assert!(complete.risks().is_empty());
    assert!(
        super::super::store(
            &root,
            &request,
            &report(),
            &assessment,
            &proof(),
            &json!({})
        )
        .unwrap()
        .is_none()
    );
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":"worker","project":root,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    state.pending = Some(Pending::from(&request));
    consume::commit_result(&store, &mut state, &request, result, None).unwrap();
    let saved = store.load().unwrap().unwrap();
    assert!(saved.pending.is_none());
    assert_eq!(checkpoint::covers(&saved, &"a".repeat(40), 1), Some(true));
    assert!(questions::pending(&root).unwrap().is_empty());
    let status: Value = source_store::read(&path.join("finished.json")).unwrap();
    assert_eq!(status["status"], "completed");
    // Recover a crash after saving the round but before its visible status.
    fs::remove_file(path.join("finished.json")).unwrap();
    assert!(finish(&root, &request, &report(), &assessment).unwrap());
    let recovered: Value = source_store::read(&path.join("finished.json")).unwrap();
    assert_eq!(recovered["status"], "completed");
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

fn completed_fixture() -> (std::path::PathBuf, Store, Request, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "kah-complete-followup-{}-{}",
        std::process::id(),
        crate::relay_store::nonce()
    ));
    let store = Store::delivery_at(&root, true).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("requests")).unwrap();
    let request = Request {
        id: "review-final".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({"verification_followup":{"stage":"checks_complete"}}),
        documents: vec![],
        created_ms: 0,
    };
    source_store::save(
        &root.join("requests"),
        "review-final.json",
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    let round = start(&root, &request).unwrap();
    advance(
        &root,
        &round.binding,
        Step::Evidence(vec![pointer("source")]),
    )
    .unwrap();
    advance(
        &root,
        &round.binding,
        Step::Builder(BuilderReceipt {
            request: pointer(&round.builder_receipt()),
            response: BuilderOutcome::Evidence {
                response: pointer("response"),
            },
        }),
    )
    .unwrap();
    let path = directory(&root, &round);
    kah_core::run_store::ensure_private_directory(&path).unwrap();
    source_store::save(&path, "job.json", &serde_json::to_value(job()).unwrap()).unwrap();
    source_store::save(
        &path,
        "checks.json",
        &serde_json::to_value(proof()).unwrap(),
    )
    .unwrap();

    (root, store, request, path)
}

#[test]
fn the_final_round_accounts_for_new_positive_uncertainty_without_a_second_ask() {
    let mut final_report = report();
    let mut added = final_report.decision_units.as_ref().unwrap().requirements[0].clone();
    added.id = "evidence_response".into();
    added.name = "Answer the evidence ask".into();
    final_report
        .decision_units
        .as_mut()
        .unwrap()
        .requirements
        .push(added);
    let risk = final_report.verification_risks.as_ref().unwrap()[0].clone();
    let mut added_risk = risk;
    added_risk.requirement_id = "evidence_response".into();
    final_report
        .verification_risks
        .as_mut()
        .unwrap()
        .push(added_risk);
    let (verified, risks) = items(
        &round(),
        &job(),
        &final_report,
        &json!({"needs_thinker":["intent","evidence_response"]}),
        &pointer("checks"),
        &pointer("decision"),
    )
    .unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(risks.len(), 2);
    assert!(
        risks
            .iter()
            .any(|r| r.requirement_id == "evidence_response")
    );
}

#[test]
fn final_report_risk_omission_is_correctable_by_the_same_thinker() {
    let mut missing = report();
    missing.verification_risks = None;
    let error = checked_items(
        &round(),
        &job(),
        &missing,
        &json!({"needs_thinker":["intent"]}),
        &pointer("checks"),
        &pointer("decision"),
    )
    .unwrap_err();
    let origin = crate::observer_review::failure::origin(error.as_ref());
    assert_eq!(
        origin,
        crate::observer_review::failure::Origin::ReportValidation
    );
    assert!(
        error
            .to_string()
            .starts_with("REVIEW_FOLLOWUP_RISK_MISSING")
    );
    assert!(crate::observer_review::failure::correctable(
        origin,
        &error.to_string()
    ));
}

#[test]
fn non_requirement_uncertainty_finishes_review_without_risk_admission() {
    let (root, store, request, path) = completed_fixture();
    assert!(
        finish(
            &root,
            &request,
            &report(),
            &json!({"passed":null,"needs_user":[],"needs_thinker":["proportion","steering"]})
        )
        .unwrap()
    );
    let saved: Value = source_store::read(&path.join("finished.json")).unwrap();
    assert_eq!(saved["status"], "unconfirmed_observation");
    assert!(completed(&root, &request).unwrap().is_none());
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn not_due_outcome_is_accounted_without_claiming_verified_or_opening_risk() {
    let (verified, risks) = items(
        &round(),
        &job(),
        &report(),
        &json!({"needs_thinker":[],"not_due":["integration"]}),
        &pointer("checks"),
        &pointer("decision"),
    )
    .unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].requirement_id, "intent");
    assert!(risks.is_empty());
}

#[test]
fn returning_to_a_scope_republishes_existing_checks_without_repeating_them() {
    let (root, store, mut request, path) = completed_fixture();
    let r = round();
    assert!(super::super::queue_checks(&root, &path, &r, &request, &job(), &proof()).unwrap());
    let old = fs::read_dir(root.join("queued"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    kah_core::run_store::ensure_private_directory(&root.join("covered-triggers")).unwrap();
    fs::rename(
        &old,
        root.join("covered-triggers").join(old.file_name().unwrap()),
    )
    .unwrap();
    request.id = "review-returned-sha".into();
    assert!(super::super::queue_checks(&root, &path, &r, &request, &job(), &proof()).unwrap());
    assert!(!super::super::queue_checks(&root, &path, &r, &request, &job(), &proof()).unwrap());
    let new = fs::read_dir(root.join("queued"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let wake: super::super::super::super::queue::Wake = source_store::read(&new).unwrap();
    assert_eq!(wake.trigger["verification_followup"]["round"], r.id);
    assert_eq!(
        wake.trigger["verification_followup"]["proof"]["head"],
        r.binding.head
    );
    assert!(
        root.join("covered-triggers")
            .join(old.file_name().unwrap())
            .exists()
    );
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
