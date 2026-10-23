//! VF14 control replay, using a labelled synthetic receipt; no agent/transport.
use super::*;
// One shared builder; each test module includes its own copy.
#[allow(clippy::duplicate_mod)]
#[path = "synthetic_gate.rs"]
mod synthetic;

#[test]
fn a_v12_plan_enters_the_single_round_without_replaying_its_native_body() {
    let value = synthetic::verifier("worker-gate");
    let mut request: Request = serde_json::from_value(value["request"].clone()).unwrap();
    let report: Report = serde_json::from_value(value["report"].clone()).unwrap();
    let state = serde_json::from_value(value["state"].clone()).unwrap();
    let root = std::env::temp_dir().join(format!("vf14-planned-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_save_directory(&root.join("sources")).unwrap();
    crate::observer_review::source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    let round = super::followup::start(&root, &request).unwrap();
    let proof = DeliveryProof {
        head: round.binding.head.clone(),
        fresh_checkout: true,
        tests: vec![],
        artifacts: vec![],
        recorded_checks_absent: true,
    };
    request.trigger["interactive_gate_verification"] = json!({"proof":proof});
    let item = report
        .verification_contracts
        .as_ref()
        .unwrap()
        .iter()
        .find(|i| i.requirement_id == "verification-pass-line")
        .unwrap();
    let Method::TargetedCheck { command } = &item.method else {
        panic!("the fixture plan changed")
    };
    seed_receipt(&root, &round, item, command);
    let mut coverage = json!({"covered":false});
    proposed_target(&report, item, &mut coverage);
    let result = one(&root, &state, &round, item, coverage, &proof, &request).unwrap();
    assert_eq!(result.check.unwrap().command, *command);
    assert!(!result.verified, "no Jev coverage verdict was supplied");
    missing_plan(&report, &root, &state, &round, &proof, &request);
    assert_eq!(
        super::followup::existing(&root, &request)
            .unwrap()
            .unwrap()
            .id,
        round.id
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn missing_plan(
    report: &Report,
    root: &Path,
    state: &crate::observer_review::state::State,
    round: &VerificationRound,
    proof: &DeliveryProof,
    request: &Request,
) {
    let mut missing = report
        .verification_contracts
        .as_ref()
        .unwrap()
        .iter()
        .find(|i| i.requirement_id == "normalize-behavior")
        .unwrap()
        .clone();
    missing.method = Method::NeedsCheck {
        why: "REVIEW_CHECK_UNVERIFIED stage=verification: no v12 planned check".into(),
    };
    let mut result = one(root, state, round, &missing, json!({}), proof, request).unwrap();
    assert!(!result.verified);
    assert!(result.check.is_none());
    terminal_coverage(report, &json!({"violations":[]}), &mut result);
    assert!(
        result.semantic_risk_admissible,
        "no plan is a named risk after the round, never a pass"
    );
}

fn seed_receipt(root: &Path, round: &VerificationRound, item: &Item, command: &str) {
    crate::observer_review::source_store::save(
        root,
        &format!("{}-reserved.json", item.requirement_id),
        &json!({"binding":round.binding,"item":item.requirement_id,"command":command}),
    )
    .unwrap();
    let path =
        crate::exact_verification::receipt_path(&root.join(format!("{}.log", item.requirement_id)));
    crate::unattended_report::save(root, path.file_name().unwrap().to_str().unwrap(),
        &json!({"receipt":{"command":command,"commit":round.binding.head,"success":true,
        "exit_code":0,"elapsed_ms":0,"output":"SYNTHETIC_CONTROL_RECEIPT\n","output_truncated":false},"changed":false})).unwrap();
}
