//! Explicit-gate coverage, verification and approval boundaries.
//! Pure host routing and recorded judgments: no provider or native sessions.
use kah_cli::observer_review::{
    gate_approval, option1, source_store,
    state::{Purpose, Request},
};
use kah_observer::{decision_units, judge::Judgment, review_evidence::Report};
use serde_json::{Value, json};

fn cumulative() -> Request {
    Request {
        id: "review-cumulative".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":3,"goal":"Deliver pipeline"}),
        trigger: json!({"option1":{"stage":"checkpoint","original_trigger":{"events":[{"kind":"user_gate","source":"host_cli"}]}}}),
        documents: vec![],
        created_ms: 0,
    }
}
fn typed() -> Request {
    let mut captured = cumulative();
    captured.id = "review-typed".into();
    captured.trigger = json!({});
    let request = option1::explicit_gate::prepare(&cumulative(), captured, &[]).unwrap();
    assert!(!option1::protocol::active(&request));
    assert!(!option1::admission::applicable(
        &request.purpose,
        &json!({"captured_snapshot":request})
    ));
    assert_eq!(request.trigger["decision_protocol_version"], 12);
    request
}
fn report(req: &Request, decision: &str) -> Report {
    serde_json::from_value(json!({"request_id":req.id,"findings":[],"decision_units":{"version":12,"requirements":[{
        "id":"integration","name":"Integrated pipeline","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "applicability":{"due":true,"unconditional":false,"phase_completion":true,"reasoning":"Explicit controller gate event.","evidence_citations":[0]},
        "judgment":{"kind":"semantic","decision":decision,"reasoning":"Retained exact-SHA code and test evidence.","evidence_citations":[0]}
    }]}})).unwrap()
}
fn assessment(report: &Report, support: f64) -> Value {
    let judgment:Judgment=serde_json::from_value(json!({"backend":"jev","model":"recorded","state_digest":"test",
        "answers":{"requirement.integration":{"type":"choice","choice":"supported","probabilities":{"supported":support,"contradicted":0.01,"unclear":1.0-support-0.01},"confidence":0.95}},"metrics":kah_observer::judge::Metrics::default(),"fallback_reason":null})).unwrap();
    let result = decision_units::assess_with_phase_due(
        report.decision_units.as_ref().unwrap(),
        &judgment,
        true,
    );
    json!({"passed":if result.passed{json!(true)}else if !result.violations.is_empty(){json!(false)}else{Value::Null},
        "violations":result.violations,"needs_user":result.needs_user,"needs_thinker":result.needs_thinker})
}
fn proof() -> gate_approval::DeliveryProof {
    serde_json::from_value(json!({"head":"a".repeat(40),"fresh_checkout":true,"tests":[{"command":"node integration.cjs","commit":"a".repeat(40),"exit_code":0,"success":true}],
        "artifacts":[{"path":"PLAN.md","baseline_blob":"b".repeat(40),"delivered_blob":"b".repeat(40),"delivered_sha":"a".repeat(40)}]})).unwrap()
}
fn round(root: &std::path::Path, request: &Request) {
    round_with_proof(root, request, proof());
}
fn round_with_proof(
    root: &std::path::Path,
    request: &Request,
    proof: gate_approval::DeliveryProof,
) {
    use gate_approval::followup::*;
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    let round = start(root, request).unwrap();
    let ptr = |id: &str| EvidencePointer {
        receipt: id.into(),
        record: format!("private:{id}"),
        digest: "d".repeat(64),
    };
    advance(root, &round.binding, Step::Evidence(vec![ptr("existing")])).unwrap();
    advance(
        root,
        &round.binding,
        Step::Builder(BuilderReceipt {
            request: ptr(&round.builder_receipt()),
            response: BuilderOutcome::Evidence {
                response: ptr("response"),
            },
        }),
    )
    .unwrap();
    advance(
        root,
        &round.binding,
        Step::Checks(CheckReceipt {
            receipt: ptr(&round.checks_receipt()),
            proof,
            verified: vec![],
            risks: vec![UnverifiedRisk {
                requirement_id: "integration".into(),
                unverified: "Provider interaction remains unverified.".into(),
                tried: vec![Attempt {
                    what: "Read exact-SHA check inventory and native evidence.".into(),
                    receipt: round.checks_receipt(),
                }],
                acceptance_risk: "The unverified provider interaction could fail.".into(),
                evidence: vec![ptr("existing")],
            }],
        }),
    )
    .unwrap();
}
fn fixture() -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("explicit-gate-{}", kah_cli::relay_store::nonce()));
    for name in ["requests", "sources"] {
        kah_core::run_store::ensure_private_directory(&root.join(name)).unwrap();
    }
    root
}
#[test]
fn proven_explicit_gate_enters_typed_policy_and_asks_nothing() {
    let request = typed();
    let report = report(&request, "satisfied");
    let assessment = assessment(&report, 0.98);
    assert_eq!(assessment["passed"], true);
    let root = fixture();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    assert!(
        gate_approval::store(&root, &request, &report, &assessment, &proof(), &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn unconfirmed_explicit_gate_after_one_round_has_one_named_question() {
    let request = typed();
    let report = report(&request, "satisfied");
    let assessment = assessment(&report, 0.70);
    let root = fixture();
    round(&root, &request);
    let first = gate_approval::store(&root, &request, &report, &assessment, &proof(), &json!({}))
        .unwrap()
        .unwrap();
    let second = gate_approval::store(&root, &request, &report, &assessment, &proof(), &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(first["id"], second["id"]);
    assert_eq!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .len(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn confirmed_violation_blocks_explicit_gate_even_after_a_round() {
    let request = typed();
    let report = report(&request, "violated");
    let assessment = assessment(&report, 0.98);
    assert_eq!(assessment["passed"], false);
    assert_eq!(assessment["violations"], json!(["integration"]));
    let root = fixture();
    round(&root, &request);
    assert!(
        gate_approval::store(&root, &request, &report, &assessment, &proof(), &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn interactive(exit: Option<i32>) -> (Request, gate_approval::DeliveryProof) {
    let mut req = typed();
    let artifacts = proof().artifacts;
    let commands = if exit.is_some() {
        vec!["node integration.cjs".into()]
    } else {
        vec![]
    };
    let proof = gate_approval::interactive::verify_commands(
        req.head.as_deref().unwrap(),
        &commands,
        artifacts,
        |_, command| {
            Ok(gate_approval::TestReceipt {
                command: command.into(),
                commit: req.head.clone().unwrap(),
                exit_code: exit,
                success: exit == Some(0),
                // Native bodies are evidence-only. This
                // receipt represents the separately planned candidate check.
                output: "Planned isolated check output".into(),
            })
        },
    )
    .unwrap();
    req.trigger["interactive_gate_verification"] = json!({"proof":proof});
    (req, proof)
}
#[test]
fn interactive_passing_planned_check_keeps_a_proven_gate_question_free() {
    let (req, proof) = interactive(Some(0));
    assert!(proof.passes(req.head.as_deref().unwrap()));
    let report = report(&req, "satisfied");
    let mut result = assessment(&report, 0.98);
    gate_approval::interactive::guard(&req, &mut result).unwrap();
    assert_eq!(result["passed"], true);
    let root = fixture();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", req.id),
        &json!(req),
    )
    .unwrap();
    assert!(
        gate_approval::store(&root, &req, &report, &result, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn interactive_failing_planned_check_blocks_even_a_supported_thinker() {
    let (req, proof) = interactive(Some(1));
    assert!(!proof.passes(req.head.as_deref().unwrap()));
    let report = report(&req, "satisfied");
    let mut result = assessment(&report, 0.98);
    gate_approval::interactive::guard(&req, &mut result).unwrap();
    assert_eq!(result["passed"], false);
    assert_eq!(result["violations"], json!(["exact_sha_tests"]));
    let root = fixture();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", req.id),
        &json!(req),
    )
    .unwrap();
    assert!(
        gate_approval::store(&root, &req, &report, &result, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn interactive_no_recorded_checks_waits_on_one_named_risk_never_a_pass() {
    let (req, proof) = interactive(None);
    assert!(!proof.passes(req.head.as_deref().unwrap()));
    let report = report(&req, "satisfied");
    let mut result = assessment(&report, 0.98);
    gate_approval::interactive::guard(&req, &mut result).unwrap();
    assert!(result["passed"].is_null());
    let root = fixture();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", req.id),
        &json!(req),
    )
    .unwrap();
    // Missing recorded checks cannot request approval before the verification round.
    assert!(
        gate_approval::store(&root, &req, &report, &result, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .is_empty()
    );
    round_with_proof(&root, &req, proof.clone());
    let first = gate_approval::store(&root, &req, &report, &result, &proof, &json!({}))
        .unwrap()
        .unwrap();
    let repeated = gate_approval::store(&root, &req, &report, &result, &proof, &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(first["id"], repeated["id"]);
    assert_eq!(
        kah_cli::observer_review::questions::pending(&root)
            .unwrap()
            .len(),
        1
    );
    assert!(
        gate_approval::message(&first)
            .unwrap()
            .contains("No recorded project check")
    );
    assert_eq!(first["verification_round"]["checks_executed"], 0);
    std::fs::remove_dir_all(root).unwrap();
}
