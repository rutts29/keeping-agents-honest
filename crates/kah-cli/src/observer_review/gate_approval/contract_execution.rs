//! Exact-SHA receipts for each planned item; reservations prevent duplicate tests.
use super::followup::{Attempt, Binding, VerificationRound};
use super::*;
use kah_observer::verification_contract::{Item, Method};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemResult {
    pub binding: Binding,
    pub requirement_id: String,
    pub claim: String,
    pub plan_digest: String,
    pub verified: bool,
    pub needs_check: bool,
    pub semantic_risk_admissible: bool,
    pub coverage: Value,
    pub check: Option<TestReceipt>,
    pub tried: Vec<Attempt>,
}
pub(super) fn execute(
    root: &Path,
    state: &super::super::state::State,
    round: &VerificationRound,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
    request: &Request,
) -> Result<Vec<ItemResult>> {
    let items = plans(report);
    let directory = root.join("gate-followups").join(&round.id).join("items");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let mut results = vec![];
    for item in &items {
        let path = directory.join(format!("{}.json", item.requirement_id));
        if let Some(saved) = super::super::state::read::<ItemResult>(&path)? {
            if saved.binding != round.binding
                || saved.requirement_id != item.requirement_id
                || saved.plan_digest != plan_digest(item)?
            {
                return Err("REVIEW_CONTRACT_BINDING stage=verification".into());
            }
            results.push(saved);
            continue;
        }
        let mut coverage = assessment["contract_coverage"][&item.requirement_id].clone();
        if report.decision_units.as_ref().is_some_and(|u| {
            u.version() >= 10
                && u.requirements.iter().any(|r| {
                    r.id == item.requirement_id && matches!(r.judgment, Thinking::AtomicFact { .. })
                })
        }) {
            coverage = json!({"native_fact_verified":true});
        }
        let result = one(&directory, state, round, item, coverage, proof, request)?;
        super::super::source_store::save(
            &directory,
            path.file_name().unwrap().to_str().unwrap(),
            &serde_json::to_value(&result)?,
        )?;
        results.push(result);
    }
    Ok(results)
}
fn one(
    directory: &Path,
    state: &super::super::state::State,
    round: &VerificationRound,
    item: &Item,
    coverage: Value,
    proof: &DeliveryProof,
    request: &Request,
) -> Result<ItemResult> {
    let mut result = initial(round, item, coverage.clone());
    result.plan_digest = plan_digest(item)?;
    if coverage["native_fact_verified"] == true {
        result.verified = true;
        result.tried.push(Attempt{what:format!("Compared the exact native receipt property for {}; already host-validated before judgment.",item.claim),receipt:round.checks_receipt()});
        return Ok(result);
    }
    match &item.method {
        Method::ControllerReceipt { facts } => {
            result.verified = super::contract_facts::verified(request, proof, facts);
            result.tried.push(Attempt {
                what: format!(
                    "Compared the controller's {:?} receipts for {} against SHA {}.",
                    facts, item.claim, round.binding.head
                ),
                receipt: round.checks_receipt(),
            });
        }
        Method::ExistingCheck { test_index } => {
            let check = proof
                .tests
                .get(*test_index)
                .ok_or("REVIEW_CONTRACT_RECEIPT stage=verification")?
                .clone();
            result.verified = coverage["covered"] == true && passes(&check, &round.binding.head);
            result.tried.push(attempt(item, &check, round));
            result.check = Some(check);
        }
        Method::TargetedCheck { command } if coverage["covered"] == true => {
            result.check = targeted(directory, state, round, item, command)?;
            result.verified = result
                .check
                .as_ref()
                .is_some_and(|c| passes(c, &round.binding.head));
            if let Some(check) = &result.check {
                result.tried.push(attempt(item, check, round));
            }
        }
        Method::SemanticRisk { .. } => {
            result.semantic_risk_admissible = coverage["semantic_risk_admissible"] == true;
        }
        _ => {}
    }
    result.needs_check = !result.verified && !result.semantic_risk_admissible;
    Ok(result)
}
fn passes(check: &TestReceipt, head: &str) -> bool {
    check.commit == head && check.exit_code == Some(0) && check.success
}
fn attempt(item: &Item, check: &TestReceipt, round: &VerificationRound) -> Attempt {
    Attempt {
        what: format!(
            "For {} ran {} on exact SHA {}; exit {:?}, passed {}. Full output is in the item receipt.",
            item.claim, check.command, check.commit, check.exit_code, check.success
        ),
        receipt: round.checks_receipt(),
    }
}
fn targeted(
    directory: &Path,
    state: &super::super::state::State,
    round: &VerificationRound,
    item: &Item,
    command: &str,
) -> Result<Option<TestReceipt>> {
    let reserved = directory.join(format!("{}-reserved.json", item.requirement_id));
    if let Some(saved) = super::super::state::read::<Value>(&reserved)? {
        if saved != json!({"binding":round.binding,"item":item.requirement_id,"command":command}) {
            return Err(
                "REVIEW_CONTRACT_BINDING stage=verification: reserved command changed".into(),
            );
        }
        return Ok(None);
    }
    super::super::source_store::save(
        directory,
        reserved.file_name().unwrap().to_str().unwrap(),
        &json!({"binding":round.binding,"item":item.requirement_id,"command":command}),
    )?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=verification")?);
    let candidate = super::checks::trusted_candidate(state)?;
    let (receipt, changed) = crate::exact_verification::execute(
        &home,
        &candidate,
        &round.binding.head,
        super::checks::options(&state.worker.session)?,
        command,
        &directory.join(format!("{}.log", item.requirement_id)),
        || Ok(false),
    )?;
    Ok(Some(TestReceipt {
        command: receipt.command,
        commit: receipt.commit,
        exit_code: receipt.exit_code,
        success: receipt.success && !changed,
        output: receipt.output,
    }))
}

fn initial(round: &VerificationRound, item: &Item, coverage: Value) -> ItemResult {
    ItemResult {
        binding: round.binding.clone(),
        requirement_id: item.requirement_id.clone(),
        claim: item.claim.clone(),
        plan_digest: String::new(),
        verified: false,
        needs_check: false,
        semantic_risk_admissible: false,
        coverage: coverage.clone(),
        check: None,
        tried: vec![
            Attempt {
                what: format!(
                    "Read checked source citations {:?} for {}: {}",
                    item.evidence_citations, item.requirement_id, item.claim
                ),
                receipt: round.evidence.as_ref().unwrap()[0].receipt.clone(),
            },
            Attempt {
                what: format!(
                    "Asked the worker once for evidence about {} on {}.",
                    item.claim, round.binding.head
                ),
                receipt: round.builder_receipt(),
            },
        ],
    }
}

fn plans(report: &Report) -> Vec<Item> {
    let mut items = report.verification_contracts.clone().unwrap_or_else(|| {
        if report
            .decision_units
            .as_ref()
            .is_some_and(|u| u.version() >= 10)
        {
            report
                .decision_units
                .as_ref()
                .unwrap()
                .requirements
                .iter()
                .map(|r| Item {
                    requirement_id: r.id.clone(),
                    claim: r.name.clone(),
                    evidence_citations: vec![],
                    coverage_reasoning: "No per-item verification plan supplied.".into(),
                    method: Method::NeedsCheck {
                        why: "Supply a receipt or targeted check covering this exact obligation."
                            .into(),
                    },
                })
                .collect()
        } else {
            vec![]
        }
    });
    if report
        .decision_units
        .as_ref()
        .is_some_and(|u| u.version() >= 10)
    {
        for r in &report.decision_units.as_ref().unwrap().requirements {
            if !items.iter().any(|item| item.requirement_id == r.id) {
                items.push(Item {
                    requirement_id: r.id.clone(),
                    claim: r.name.clone(),
                    evidence_citations: vec![],
                    coverage_reasoning: "No per-item check supplied.".into(),
                    method: Method::NeedsCheck {
                        why: "Provide a covering receipt or targeted check.".into(),
                    },
                });
            }
        }
    }

    items
}

fn plan_digest(item: &Item) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(item)?)
    ))
}
