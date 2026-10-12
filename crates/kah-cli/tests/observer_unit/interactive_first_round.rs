//! Whole retained VF12 control replay; no native launch or transport.
use super::*;

fn retained() -> (Request, Report, DeliveryProof, Value) {
    let v: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-v12-first-round/retained-input.json"
    ))
    .unwrap();
    (
        serde_json::from_value(v["request"].clone()).unwrap(),
        serde_json::from_value(v["report"].clone()).unwrap(),
        serde_json::from_value(v["proof"].clone()).unwrap(),
        v["assessment"].clone(),
    )
}

fn proven_receipt() -> DeliveryProof {
    let v: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-v12-first-round/retained-input.json"
    ))
    .unwrap();
    let (_, _, mut proof, _) = retained();
    let check = &v["recorded_controller_check"];
    proof.tests.push(TestReceipt {
        command: check["command"].as_str().unwrap().into(),
        commit: check["commit"].as_str().unwrap().into(),
        exit_code: Some(check["exit_code"].as_i64().unwrap() as i32),
        success: check["success"].as_bool().unwrap(),
        output: check["output"].as_str().unwrap().into(),
    });
    proof.recorded_checks_absent = false;
    assert!(proof.passes(&proof.head));
    proof
}

#[test]
fn retained_initial_v12_plan_never_builds_a_premature_question() {
    let (request, report, proof, assessment) = retained();
    count_only(&request, &report);
    let root =
        std::env::temp_dir().join(format!("kah-first-round-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    assert!(
        super::super::store(&root, &request, &report, &assessment, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(!root.join("questions").exists());
    fs::remove_dir_all(root).unwrap();
}

fn count_only(request: &Request, report: &Report) {
    use crate::observer_review::{checkpoint, decisions};
    use kah_observer::{context_budget::Calibration, decision_units, judge::Questions};
    let v: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-v12-first-round/retained-input.json"
    ))
    .unwrap();
    let calibration: Calibration = serde_json::from_value(v["calibration"].clone()).unwrap();
    let verified = decisions::verify_evidence(request, report)
        .unwrap()
        .unwrap();
    let mut input = checkpoint::decision_context(
        &request.intent,
        &request.trigger,
        request.head.as_deref(),
        &json!([]),
        json!([]),
    );
    input["decision"] = decision_units::ReviewUnits::named_state(&verified).unwrap();
    let mut questions = decision_units::questions(report.decision_units.as_ref().unwrap());
    super::super::prepare_contracts(request, report, &mut input, &mut questions).unwrap();
    let plans = questions
        .iter()
        .map(|(key, q)| {
            let state = decisions::scoped::report_context(&input, key).unwrap();
            let plan = calibration
                .plan(&state, &Questions::from([(key.clone(), q.clone())]))
                .unwrap();
            json!({"key":key,"plan":plan})
        })
        .collect::<Vec<_>>();
    let tokens: f64 = plans
        .iter()
        .map(|p| p["plan"]["estimated_request_tokens"].as_f64().unwrap())
        .sum();
    println!(
        "COUNT_ONLY {}",
        json!({"estimated":true,"requests":plans.len(),
        "estimated_input_tokens":tokens,"estimated_usd":tokens*0.042/1_000_000.0,
        "planning_only":true,"real_jev_calls":0,"per_request":plans})
    );
}

#[test]
fn retained_unclear_items_have_named_fact_based_risks_and_good_gate_has_none() {
    let (_, report, _, mut assessment) = retained();
    let risks = missing::risks(&report, &assessment).unwrap();
    assert_eq!(risks.len(), 4);
    assert!(
        risks[0]["unverified"]
            .as_str()
            .unwrap()
            .contains("No recorded")
    );
    for risk in &risks[1..] {
        let id = risk["requirement_id"].as_str().unwrap();
        let item = report
            .decision_units
            .as_ref()
            .unwrap()
            .requirements
            .iter()
            .find(|r| r.id == id)
            .unwrap();
        assert!(risk["unverified"].as_str().unwrap().contains(&item.name));
        assert!(!risk["acceptance_risk"].as_str().unwrap().is_empty());
    }
    assert!(
        super::risk_text(&report, "unknown-item")
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_FOLLOWUP_RISK_MISSING stage=verification")
    );
    assessment["passed"] = true.into();
    assessment["needs_thinker"] = json!([]);
    let root = std::env::temp_dir().join(format!("kah-proven-{}", crate::relay_store::nonce()));
    let (request, report, _, _) = retained();
    let proof = proven_receipt();
    assert!(
        super::super::store(&root, &request, &report, &assessment, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    assert!(!root.join("questions").exists());
}

#[test]
fn retained_usable_plan_stages_one_round_before_any_question() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
    let (mut request, report, proof, assessment) = retained();
    request.worker = format!("first-round-{}", crate::relay_store::nonce());
    let store = Store::open(&request.worker).unwrap();
    let root = store.directory.clone();
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", request.id),
        &json!(request),
    )
    .unwrap();
    assert!(
        super::super::producer::begin_with(&store, &request, &report, &assessment, || Ok(()))
            .unwrap()
    );
    let round = super::super::followup::start(&root, &request).unwrap();
    assert_eq!(round.stage, super::super::followup::Stage::Builder);
    assert!(
        root.join("gate-followups")
            .join(&round.id)
            .join("job.json")
            .is_file()
    );
    assert!(
        super::super::store(&root, &request, &report, &assessment, &proof, &json!({}))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        super::super::followup::start(&root, &request).unwrap().id,
        round.id
    );
    assert!(!root.join("questions").exists());
    complete_unclear_round(&root, &request, &report, &proof, &assessment);
    assert!(
        !super::super::producer::begin_with(&store, &request, &report, &assessment, || panic!(
            "a completed round must never schedule another native wake"
        ))
        .unwrap()
    );
    assert_completed_questions(&root, &request, &report, &proof, &assessment);
    drop(store);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn assert_completed_questions(
    root: &Path,
    request: &Request,
    report: &Report,
    proof: &DeliveryProof,
    assessment: &Value,
) {
    let question = super::super::store(root, request, report, assessment, proof, &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(question["unverified_risks"].as_array().unwrap().len(), 4);
    assert!(
        question["summary"]
            .as_str()
            .unwrap()
            .contains("No recorded project check")
    );
    let mut proven = assessment.clone();
    proven["passed"] = true.into();
    proven["needs_thinker"] = json!([]);
    assert!(
        super::super::store(
            root,
            request,
            report,
            &proven,
            &proven_receipt(),
            &json!({})
        )
        .unwrap()
        .is_none()
    );
}

fn complete_unclear_round(
    root: &Path,
    request: &Request,
    report: &Report,
    proof: &DeliveryProof,
    assessment: &Value,
) {
    use super::super::followup::*;
    let round = start(root, request).unwrap();
    let evidence = round.evidence.clone().unwrap();
    let pointer = |name: String| EvidencePointer {
        receipt: name,
        record: "private:test-control-replay".into(),
        digest: "d".repeat(64),
    };
    advance(
        root,
        &round.binding,
        Step::Builder(BuilderReceipt {
            request: pointer(round.builder_receipt()),
            response: BuilderOutcome::Unavailable {
                reason: "Control-only replay; no native agent or transport".into(),
            },
        }),
    )
    .unwrap();
    let risks = assessment["needs_thinker"].as_array().unwrap().iter().map(|id| {
        let risk = super::risk_text(report, id.as_str().unwrap()).unwrap();
        UnverifiedRisk { requirement_id: risk.requirement_id, unverified: risk.unverified,
            acceptance_risk: risk.acceptance_risk, evidence: evidence.clone(),
            tried: vec![Attempt { what: "Read the complete retained report and controller proof; no recorded project checks exist".into(), receipt: round.checks_receipt() }],
        }
    }).collect();
    advance(
        root,
        &round.binding,
        Step::Checks(CheckReceipt {
            receipt: pointer(round.checks_receipt()),
            proof: proof.clone(),
            risks,
            verified: vec![],
        }),
    )
    .unwrap();
}
