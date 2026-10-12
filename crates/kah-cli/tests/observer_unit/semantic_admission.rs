//! Frozen VF16: status/scope uncertainty cannot become an unadmitted user risk.
use super::*;
use crate::observer_review::source_store;
use std::fs;

fn retained() -> (Request, Report, Value, followup::VerificationRound) {
    let input: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-semantic-admission/retained-input.json"
    ))
    .unwrap();
    let evaluation: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-semantic-admission/retained-evaluation.json"
    ))
    .unwrap();
    (
        serde_json::from_value(input["request"].clone()).unwrap(),
        serde_json::from_value(input["report"].clone()).unwrap(),
        evaluation["assessment"].clone(),
        serde_json::from_str(include_str!(
            "../../../../docs/gates/evidence/phase3-semantic-admission/retained-round.json"
        ))
        .unwrap(),
    )
}

#[test]
fn exact_vf16_semantic_status_items_without_admission_ask_no_user_question() {
    let (mut request, report, assessment, round) = retained();
    request
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    let root = std::env::temp_dir().join(format!(
        "semantic-admission-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_save_directory(&root.join("sources")).unwrap();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    source_store::save(
        &root.join("gate-followups"),
        &format!("{}.json", round.id),
        &json!(round),
    )
    .unwrap();
    let proof = &round.checks.as_ref().unwrap().proof;
    let question = store(&root, &request, &report, &assessment, proof, &json!({})).unwrap();
    assert!(
        question.is_none(),
        "non-admitted semantic/status work stays with the observer"
    );
    assert!(assessment["passed"].is_null(), "no silent pass");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn admitted_semantic_risk_asks_exactly_one_named_question() {
    let (mut request, report, mut assessment, round) = retained();
    request
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    for id in ["req-intent", "req-drift"] {
        assessment["contract_coverage"][id]["semantic_risk_admissible"] = true.into();
    }
    let root = prepared(&request, &round);
    let proof = &round.checks.as_ref().unwrap().proof;
    let first = store(
        &root,
        &request,
        &report,
        &assessment,
        proof,
        &json!({"synthetic_admission":true}),
    )
    .unwrap()
    .unwrap();
    let second = store(
        &root,
        &request,
        &report,
        &assessment,
        proof,
        &json!({"synthetic_admission":true}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(first["id"], second["id"]);
    assert_eq!(
        crate::observer_review::questions::pending(&root)
            .unwrap()
            .len(),
        1
    );
    assert!(
        message(&first)
            .unwrap()
            .contains(request.head.as_deref().unwrap())
    );
    fs::remove_dir_all(root).unwrap();
}

fn prepared(request: &Request, round: &followup::VerificationRound) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("semantic-case-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_save_directory(&root.join("sources")).unwrap();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    source_store::save(
        &root.join("gate-followups"),
        &format!("{}.json", round.id),
        &json!(round),
    )
    .unwrap();
    let parent: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-semantic-admission/queue-parent.json"
    ))
    .unwrap();
    source_store::save(&root, "queue-order.json", &parent["order"]).unwrap();
    source_store::save(&root, "queue-metadata.json", &parent["metadata"]).unwrap();
    root
}

#[test]
fn non_admitted_case_stages_one_same_observer_resolution_then_cited_satisfaction() {
    let (mut request, report, mut assessment, round) = retained();
    request
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    let root = prepared(&request, &round);
    let next = stage_context(&root, &request, &report, &mut assessment);
    verify_context(&next, &request);
    // Explicit $0 post-resolution conclusion; native selection quality is live proof.
    assessment["needs_thinker"] = json!([]);
    assessment["unproven"] = json!([]);
    assessment["passed"] = true.into();
    assert!(
        !semantic_resolution::guard_with(&root, &next, &report, &mut assessment, |_| panic!(
            "no new round"
        ))
        .unwrap()
    );
    assert!(
        store(
            &root,
            &next,
            &report,
            &assessment,
            &round.checks.as_ref().unwrap().proof,
            &json!({"synthetic_cited_satisfaction":true})
        )
        .unwrap()
        .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

fn code_fixture(root: &Path, head: &str) -> kah_run::worktree::Worktree {
    let source = root.join("source");
    kah_core::run_store::ensure_save_directory(&source).unwrap();
    let bundle = root.join("source.bundle");
    fs::write(
        &bundle,
        include_bytes!(
            "../../../../docs/gates/evidence/phase3-semantic-admission/retained-source.bundle"
        ),
    )
    .unwrap();
    kah_run::worktree::Worktree::from_bundle(
        &source,
        &root.join("candidate"),
        &bundle,
        head,
        "run-semantic-code-fixture",
    )
    .unwrap()
}

#[test]
fn unsupported_semantic_claim_produces_one_finding_and_never_passes() {
    let (mut request, report, mut assessment, round) = retained();
    request
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    request.trigger["semantic_resolution"] = json!({"items":["req-drift"],"same_observer":true});
    let root = prepared(&request, &round);
    assessment["violations"] = json!(["req-drift"]);
    assessment["needs_thinker"] = json!([]);
    assert!(
        semantic_resolution::guard_with(&root, &request, &report, &mut assessment, |_| panic!(
            "no second check"
        ))
        .unwrap()
    );
    assert_eq!(assessment["passed"], false);
    assert!(
        semantic_resolution::guard_with(&root, &request, &report, &mut assessment, |_| panic!(
            "no requeue"
        ))
        .unwrap()
    );
    assert_eq!(
        crate::observer_review::reminder_queue::pending(&root)
            .unwrap()
            .len(),
        1
    );
    assert!(!root.join("queued").exists());
    assert!(
        store(
            &root,
            &request,
            &report,
            &assessment,
            &round.checks.as_ref().unwrap().proof,
            &json!({"synthetic_unsupported":true})
        )
        .unwrap()
        .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

fn stage_context(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &mut Value,
) -> Request {
    let tree = code_fixture(root, request.head.as_deref().unwrap());
    let attach = |snapshot: &mut Request| {
        semantic_resolution::append_code(&tree, request.head.as_deref().unwrap(), snapshot)?;
        snapshot.trigger["semantic_resolution"]["observer_session"] =
            "00000000-0000-4000-8000-100000000009".into();
        Ok(())
    };
    assert!(semantic_resolution::guard_with(root, request, report, assessment, attach).unwrap());
    assert!(assessment["passed"].is_null());
    assert!(
        semantic_resolution::guard_with(root, request, report, assessment, |_| panic!(
            "no second packet"
        ))
        .unwrap()
    );
    assert_eq!(fs::read_dir(root.join("queued")).unwrap().count(), 1);
    let path = fs::read_dir(root.join("semantic-resolutions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    source_store::read(&path).unwrap()
}

fn verify_context(next: &Request, request: &Request) {
    assert_eq!(next.worker, request.worker);
    assert_eq!(next.head, request.head);
    assert_eq!(
        next.trigger["semantic_resolution"]["observer_session"],
        "00000000-0000-4000-8000-100000000009"
    );
    for id in [
        "completion-due",
        "controller-evidence",
        "controller-receipts",
    ] {
        let document = next.documents.iter().find(|d| d.id == id).unwrap();
        let body: Value = serde_json::from_str(&document.text).unwrap();
        assert_eq!(body["request"], next.id);
    }
    assert_eq!(
        next.trigger["semantic_resolution"]["verification_rounds"],
        1
    );
    assert_eq!(
        next.documents
            .iter()
            .filter(|d| d.kind == "committed_source")
            .count(),
        7
    );
    for name in [
        "normalize.cjs",
        "serialize.cjs",
        "pipeline.cjs",
        "verification.cjs",
        "PLAN.md",
    ] {
        assert!(
            next.documents
                .iter()
                .any(|d| d.uri.ends_with(name) && d.kind == "committed_source")
        );
    }
}
