#[allow(dead_code)]
mod checkpoint_replay;
use kah_cli::observer_review::{
    gate_approval::{self, followup::*},
    source_store,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
use std::fs;
fn pointer(name: &str) -> EvidencePointer {
    EvidencePointer {
        receipt: name.into(),
        record: format!("private:{name}"),
        digest: "d".repeat(64),
    }
}
fn proof() -> gate_approval::DeliveryProof {
    serde_json::from_value(json!({"head":"a".repeat(40),"fresh_checkout":true,"tests":[{"command":"node verification.cjs","commit":"a".repeat(40),"exit_code":0,"success":true}],
        "artifacts":[{"path":"PLAN.md","baseline_blob":"b".repeat(40),"delivered_blob":"b".repeat(40),"delivered_sha":"a".repeat(40)}]})).unwrap()
}
fn fixture() -> (std::path::PathBuf, Request, Report, Value) {
    let root = std::env::temp_dir().join(format!(
        "kah-v12-nondiff-risk-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    kah_core::run_store::ensure_private_directory(&root.join("requests")).unwrap();
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    let request = Request {
        id: "review-v12-risk".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    complete_round(&root, &request);
    let report:Report=serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":12,"requirements":[{
        "id":"no-push-deploy","name":"Do not push or deploy","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"No violation is established; complete run-action coverage remains unconfirmed","evidence_citations":[0]}}]},
        "verification_contracts":[{"requirement_id":"no-push-deploy","claim":"Do not push or deploy","evidence_citations":[0],"coverage_reasoning":"Exact-SHA code checks passed; run-action coverage remains unconfirmed","method":{"kind":"existing_check","test_index":0}}]})).unwrap();
    let followup = completed(&root, &request).unwrap().unwrap();
    let assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["no-push-deploy"],"contract_coverage":{"no-push-deploy":{"covered":false}}});
    drop(followup);
    (root, request, report, assessment)
}
#[test]
fn nondiff_uncertainty_after_one_round_reaches_the_named_risk_question() {
    // The tag review leaves this run-action prohibition unclear, and code
    // alone cannot check it.
    let tag = checkpoint_replay::tag();
    assert!(
        tag["report"]["result"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["requirement"] == "no-push-deploy" && r["status"] == "unclear")
    );
    assert_eq!(
        tag["input"]["rules"]["no-push-deploy"]["literal_check"],
        false
    );
    let (root, request, report, mut assessment) = fixture();
    let followup = completed(&root, &request).unwrap().unwrap();
    assert!(gate_approval::eligible_after_followup(
        &request,
        &report,
        &assessment,
        &proof(),
        &followup
    ));
    let first = gate_approval::store(&root, &request, &report, &assessment, &proof(), &json!({}))
        .unwrap()
        .unwrap();
    let repeated =
        gate_approval::store(&root, &request, &report, &assessment, &proof(), &json!({}))
            .unwrap()
            .unwrap();
    assert_eq!(first["id"], repeated["id"]);
    assert!(first["summary"].as_str().unwrap().contains("Accept gate"));
    assert_eq!(
        first["unverified_risks"][0]["requirement_id"],
        "no-push-deploy"
    );
    assert_eq!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .len(),
        1
    );
    assessment["violations"] = json!(["no-push-deploy"]);
    assert!(!gate_approval::eligible_after_followup(
        &request,
        &report,
        &assessment,
        &proof(),
        &followup
    ));
    fs::remove_dir_all(root).unwrap();
}

fn complete_round(root: &std::path::Path, request: &Request) {
    let round = start(root, request).unwrap();
    advance(
        root,
        &round.binding,
        Step::Evidence(vec![pointer("source")]),
    )
    .unwrap();
    advance(
        root,
        &round.binding,
        Step::Builder(BuilderReceipt {
            request: pointer(&round.builder_receipt()),
            response: BuilderOutcome::Evidence {
                response: pointer("native-answer"),
            },
        }),
    )
    .unwrap();
    advance(
        root,
        &round.binding,
        Step::Checks(CheckReceipt {
            receipt: pointer(&round.checks_receipt()),
            proof: proof(),
            verified: vec![],
            risks: vec![UnverifiedRisk {
                requirement_id: "no-push-deploy".into(),
                unverified: "Whether any push or deployment occurred remains unconfirmed from code"
                    .into(),
                acceptance_risk: "Accepting leaves the named run-action prohibition unconfirmed"
                    .into(),
                tried: vec![Attempt {
                    what: "Read code, asked once, and ran same-SHA test".into(),
                    receipt: round.checks_receipt(),
                }],
                evidence: vec![pointer("source")],
            }],
        }),
    )
    .unwrap();
}
