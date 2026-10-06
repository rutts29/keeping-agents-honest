use kah_cli::observer_review::{
    gate_approval,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::Report;
use serde_json::json;
fn request() -> Request {
    Request {
        id: "review-rethink".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1,"goal":"Complete phase two"}),
        trigger: json!({"reconsideration":{"request":"review-first"}}),
        documents: vec![],
        created_ms: 0,
    }
}
fn report() -> Report {
    serde_json::from_value(json!({"request_id":"review-rethink","findings":[],"decision_units":{"version":8,"requirements":[{
        "id":"intent","name":"Phase two meets intent","dimension":"intent","authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Exact committed implementation and receipts meet the phase","evidence_citations":[0]}
    }]}})).unwrap()
}
fn proof() -> gate_approval::DeliveryProof {
    serde_json::from_value(json!({"head":"a".repeat(40),"fresh_checkout":true,
        "tests":[{"command":"node integration.cjs","commit":"a".repeat(40),"exit_code":0,"success":true}],
        "artifacts":[{"path":"PLAN.md","baseline_blob":"b".repeat(40),"delivered_blob":"b".repeat(40),"delivered_sha":"a".repeat(40)}]})).unwrap()
}
fn assessment() -> serde_json::Value {
    json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["intent"]})
}
#[test]
fn only_rethought_positive_uncertainty_with_exact_delivery_can_ask_for_approval() {
    // Reconsideration alone does not complete the verification round.
    assert!(!gate_approval::eligible(
        &request(),
        &report(),
        &assessment(),
        &proof()
    ));
    let root = fixture("eligibility");
    let done = gate_approval::followup::completed(&root, &request())
        .unwrap()
        .unwrap();
    assert!(gate_approval::eligible_after_followup(
        &request(),
        &report(),
        &assessment(),
        &proof(),
        &done
    ));
    std::fs::remove_dir_all(root).unwrap();
    let mut req = request();
    req.trigger = json!({});
    assert!(!gate_approval::eligible(
        &req,
        &report(),
        &assessment(),
        &proof()
    ));
    let mut blocked = assessment();
    blocked["violations"] = json!(["integration"]);
    assert!(!gate_approval::eligible(
        &request(),
        &report(),
        &blocked,
        &proof()
    ));
    let mut failed = proof();
    failed.tests[0].exit_code = Some(1);
    assert!(!gate_approval::eligible(
        &request(),
        &report(),
        &assessment(),
        &failed
    ));
    failed = proof();
    failed.head = "c".repeat(40);
    assert!(!gate_approval::eligible(
        &request(),
        &report(),
        &assessment(),
        &failed
    ));
    failed = proof();
    failed.artifacts[0].delivered_blob = None;
    assert!(!gate_approval::eligible(
        &request(),
        &report(),
        &assessment(),
        &failed
    ));
}

#[test]
fn eligible_gate_creates_one_bound_question_with_plain_summary_and_all_evidence() {
    let root = fixture("question");
    let first=gate_approval::store(&root,&request(),&report(),&assessment(),&proof(),&json!({"backend":"jev","answers":{"requirement.intent":{"choice":"unclear","confidence":0.8}}})).unwrap().unwrap();
    let mut later = request();
    later.id = "review-later".into();
    archive(&root, &later);
    let mut later_report = report();
    later_report.request_id = later.id.clone();
    let again = gate_approval::store(
        &root,
        &later,
        &later_report,
        &assessment(),
        &proof(),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(first["id"], again["id"]);
    let pending = kah_cli::observer_review::questions::pending(&root).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["head"], "a".repeat(40));
    assert_eq!(pending[0]["request"], "review-rethink");
    let text = kah_cli::observer_review::questions::message(&pending[0]).unwrap();
    assert!(text.starts_with("[kah-builder] Accept gate "));
    assert_eq!(
        kah_codex::authorship::classify(&text, false),
        kah_core::intent_history::SourceRole::Delegate
    );
    assert!(text.contains("Risk if accepted:"));
    assert!(text.contains("or send it back for more evidence?"));
    assert!(pending[0]["jev"].to_string().contains("unclear"));
    assert!(
        pending[0]["proof"]
            .to_string()
            .contains("node integration.cjs")
    );
    assert!(!text.contains("unclear") && !text.contains("node integration.cjs"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_explicit_current_user_answer_closes_the_named_gate() {
    use kah_core::intent_history::SourceRole;
    let root = fixture("resolution");
    let q = gate_approval::store(
        &root,
        &request(),
        &report(),
        &assessment(),
        &proof(),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    let source = user_answer(q["id"].as_str().unwrap());
    let mut delegate = source.clone();
    delegate.role = SourceRole::Assistant;
    assert!(gate_approval::resolve(&root, &delegate, "Approve", &"a".repeat(40), 1, &[]).is_err());
    assert!(gate_approval::resolve(&root, &source, "Approve", &"c".repeat(40), 1, &[]).is_err());
    assert!(gate_approval::resolve(&root, &source, "Approve", &"a".repeat(40), 2, &[]).is_err());
    assert!(
        gate_approval::resolve(
            &root,
            &source,
            "Approve",
            &"a".repeat(40),
            1,
            &["integration".into()]
        )
        .is_err()
    );
    assert_eq!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .len(),
        1
    );
    let applied = gate_approval::resolve(
        &root,
        &source,
        "Approve, preserve all constraints.",
        &"a".repeat(40),
        1,
        &[],
    )
    .unwrap()
    .unwrap();
    assert_eq!(applied["status"], "answered");
    assert_eq!(applied["resolution"]["source"]["record"], source.record);
    assert_eq!(
        applied["resolution"]["verbatim"],
        "Approve, preserve all constraints."
    );
    assert!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}
fn user_answer(question: &str) -> kah_core::intent_history::SourceRef {
    use kah_core::intent_history::{
        GateDecision, QuestionAnswer, SourceOrigin, SourceProvider, SourceRef, SourceRole,
    };
    let mut source = SourceRef::new(
        SourceProvider::Codex,
        "worker",
        "relayed-user:kah-reviewer:receipt",
        SourceRole::User,
        "sha256:digest",
    );
    source.origin = Some(SourceOrigin::Relayed);
    source.relayer = Some("kah-reviewer".into());
    source.question_answer = Some(QuestionAnswer {
        question: question.into(),
        decision: GateDecision::Approve,
    });
    source
}

#[test]
fn wrong_question_does_not_close_another_gate_and_rejection_remains_user_authority() {
    use kah_core::intent_history::GateDecision;
    let root = fixture("rejection");
    let q = gate_approval::store(
        &root,
        &request(),
        &report(),
        &assessment(),
        &proof(),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    let wrong = user_answer(&format!("question-{}", "d".repeat(64)));
    assert!(gate_approval::resolve(&root, &wrong, "Approve", &"a".repeat(40), 1, &[]).is_err());
    assert_eq!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .len(),
        1
    );
    let mut source = user_answer(q["id"].as_str().unwrap());
    source.question_answer.as_mut().unwrap().decision = GateDecision::Reject;
    let result = gate_approval::resolve(
        &root,
        &source,
        "Reject: the scope is wrong.",
        &"a".repeat(40),
        1,
        &[],
    )
    .unwrap()
    .unwrap();
    assert_eq!(result["status"], "rejected");
    assert_eq!(result["resolution"]["decision"], "reject");
    assert_eq!(
        result["resolution"]["verbatim"],
        "Reject: the scope is wrong."
    );
    assert!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn captured_proof_requires_controller_run_receipt_on_the_same_sha_not_model_marker_text() {
    let mut req = request();
    req.documents
        .push(kah_observer::review_evidence::Document::new(
            "artifact-0",
            "controller_artifact_digests",
            "git:artifact",
            &serde_json::to_string(&proof().artifacts[0]).unwrap(),
        ));
    req.trigger["events"] = json!([{"kind":"run_completion","completion_key":{"commit":"a".repeat(40)},
        "verification":{"commit":"a".repeat(40),"command":"node integration.cjs","success":true,"exit_code":0,"elapsed_ms":1,"output":"PASS\n","output_truncated":false}}]);
    assert!(
        gate_approval::captured(&req)
            .unwrap()
            .unwrap()
            .passes(&"a".repeat(40))
    );
    req.trigger["events"][0]["verification"]["commit"] = json!("c".repeat(40));
    assert!(gate_approval::captured(&req).is_err());
    req.trigger = json!({"model_claim":"controller says fresh checkout PASS"});
    assert!(gate_approval::captured(&req).unwrap().is_none());
}

#[test]
fn replayed_receipt_cannot_approve_a_later_sha_after_original_resolution() {
    let root = fixture("replay");
    let q = gate_approval::store(
        &root,
        &request(),
        &report(),
        &assessment(),
        &proof(),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    let answer = user_answer(q["id"].as_str().unwrap());
    gate_approval::resolve(&root, &answer, "Approve", &"a".repeat(40), 1, &[]).unwrap();
    assert!(gate_approval::resolve(&root, &answer, "Approve", &"c".repeat(40), 1, &[]).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

// Controller-only fixtures for the user-approved one verification round.
fn archive(root: &std::path::Path, req: &Request) {
    kah_cli::observer_review::source_store::save(
        &root.join("requests"),
        &format!("{}.json", req.id),
        &serde_json::to_value(req).unwrap(),
    )
    .unwrap();
}
fn fixture(label: &str) -> std::path::PathBuf {
    use gate_approval::followup::*;
    let root = std::env::temp_dir().join(format!("kah-approval-{label}-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("requests")).unwrap();
    let mut first = request();
    first.id = "review-first".into();
    first.trigger = json!({});
    archive(&root, &first);
    archive(&root, &request());
    let round = start(&root, &request()).unwrap();
    let pointer = |receipt: &str| EvidencePointer {
        receipt: receipt.into(),
        record: format!("private:{receipt}"),
        digest: "d".repeat(64),
    };
    advance(
        &root,
        &round.binding,
        Step::Evidence(vec![pointer("existing")]),
    )
    .unwrap();
    advance(
        &root,
        &round.binding,
        Step::Builder(BuilderReceipt {
            request: pointer(&round.builder_receipt()),
            response: BuilderOutcome::Evidence {
                response: pointer("builder-response"),
            },
        }),
    )
    .unwrap();
    advance(
        &root,
        &round.binding,
        Step::Checks(CheckReceipt {
            verified: vec![],
            receipt: pointer(&round.checks_receipt()),
            proof: proof(),
            risks: vec![UnverifiedRisk {
                requirement_id: "intent".into(),
                unverified: "Intent alignment remains unconfirmed despite passing checks.".into(),
                tried: vec![Attempt {
                    what: "Read receipts, requested builder evidence, reran exact-SHA checks."
                        .into(),
                    receipt: round.checks_receipt(),
                }],
                acceptance_risk: "Accepting could advance with an unmet intent detail.".into(),
                evidence: vec![pointer("existing"), pointer("builder-response")],
            }],
        }),
    )
    .unwrap();
    root
}
