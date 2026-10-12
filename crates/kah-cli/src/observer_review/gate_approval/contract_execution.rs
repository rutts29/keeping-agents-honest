//! Exact-SHA receipts for each planned item; reservations prevent duplicate tests.
use super::followup::{Attempt, Binding, VerificationRound};
use super::*;
use kah_observer::verification_contract::{Item, Method};
use std::path::PathBuf;
mod source;

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
    pub evidence: Vec<Value>,
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
        proposed_target(report, item, &mut coverage);
        if report.decision_units.as_ref().is_some_and(|u| {
            u.version() >= 10
                && u.requirements.iter().any(|r| {
                    r.id == item.requirement_id
                        && matches!(
                            r.judgment,
                            Thinking::AtomicFact { .. } | Thinking::ControllerFact { .. }
                        )
                })
        }) {
            coverage = json!({"native_fact_verified":true});
        }
        let linked = linked_item(report, item)?;
        let mut result = if exempted(assessment, &item.requirement_id) {
            initial(round, &linked, json!({"exempt":true,"prior":coverage}))
        } else {
            one(&directory, state, round, &linked, coverage, proof, request)?
        };
        terminal_coverage(report, assessment, &mut result);
        result.plan_digest = plan_digest(item)?;
        let mut tried =
            super::contract_evidence::attempts(request, report, &linked, round, assessment)?;
        tried.extend(result.tried);
        result.tried = tried;
        result.evidence = super::contract_evidence::selected(request, report, &linked)?;
        super::super::source_store::save(
            &directory,
            path.file_name().unwrap().to_str().unwrap(),
            &serde_json::to_value(&result)?,
        )?;
        results.push(result);
    }
    Ok(results)
}
fn proposed_target(report: &Report, item: &Item, coverage: &mut Value) {
    if report
        .decision_units
        .as_ref()
        .is_some_and(|u| u.version() >= 12)
        && matches!(item.method, Method::TargetedCheck { .. })
    {
        // Judge coverage against the real receipt after executing once.
        if !coverage.is_object() {
            *coverage = json!({});
        }
        coverage["execute_reasoned_check"] = true.into();
    }
}
fn terminal_coverage(report: &Report, assessment: &Value, result: &mut ItemResult) {
    if report
        .decision_units
        .as_ref()
        .is_some_and(|u| u.version() >= 12)
        && !result.verified
        && result
            .check
            .as_ref()
            .is_none_or(|c| passes(c, &result.binding.head))
        && !exempted(assessment, &result.requirement_id)
        && assessment["violations"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && report
            .decision_units
            .as_ref()
            .unwrap()
            .requirements
            .iter()
            .any(|r| {
                r.id == result.requirement_id
                    && matches!(
                        r.judgment,
                        Thinking::Semantic {
                            decision: SemanticDecision::Satisfied,
                            ..
                        }
                    )
            })
    {
        result.semantic_risk_admissible = true;
        result.needs_check = false;
    }
}
pub(super) fn linked_item(report: &Report, item: &Item) -> Result<Item> {
    super::super::failure::validation(super::super::failure::Origin::ReportValidation, || {
        bind_item(report, item)
    })
}
fn bind_item(report: &Report, item: &Item) -> Result<Item> {
    let requirement = report
        .decision_units
        .as_ref()
        .and_then(|units| {
            units
                .requirements
                .iter()
                .find(|r| r.id == item.requirement_id)
        })
        .ok_or("REVIEW_CONTRACT_ITEM stage=verification: unknown requirement")?;
    let mut linked = item.clone();
    linked.claim = requirement.name.clone();
    Ok(linked)
}
pub(super) fn exempted(assessment: &Value, id: &str) -> bool {
    ["not_due", "deferred"].iter().any(|key| {
        assessment[*key]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|value| value == id))
    })
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
            result.verified = coverage["covered"] == true
                && super::contract_facts::verified(request, proof, facts);
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
        Method::TargetedCheck { command }
            if coverage["covered"] == true || coverage["execute_reasoned_check"] == true =>
        {
            source::validate(request, &coverage, command)?;
            result.check = targeted(directory, state, round, item, command)?;
            result.verified = coverage["covered"] == true
                && result
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
        return recovered_target(directory, item, command, &round.binding.head);
    }
    super::super::source_store::save(
        directory,
        reserved.file_name().unwrap().to_str().unwrap(),
        &json!({"binding":round.binding,"item":item.requirement_id,"command":command}),
    )?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=verification")?);
    let candidate = super::checks::trusted_candidate(state, &round.binding.head)?;
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
        tried: vec![],
        evidence: vec![],
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

fn recovered_target(
    directory: &Path,
    item: &Item,
    command: &str,
    head: &str,
) -> Result<Option<TestReceipt>> {
    let path = crate::exact_verification::receipt_path(
        &directory.join(format!("{}.log", item.requirement_id)),
    );
    let Some(saved) = super::super::state::read::<Value>(&path)? else {
        return Ok(None);
    };
    let receipt: kah_run::verification::Receipt = serde_json::from_value(saved["receipt"].clone())?;
    if receipt.command != command || receipt.commit != head {
        return Err("REVIEW_CONTRACT_BINDING stage=verification: recovered receipt changed".into());
    }
    let changed = saved["changed"]
        .as_bool()
        .ok_or("REVIEW_CONTRACT_RECEIPT stage=verification")?;
    Ok(Some(TestReceipt {
        command: receipt.command,
        commit: receipt.commit,
        exit_code: receipt.exit_code,
        success: receipt.success && !changed,
        output: receipt.output,
    }))
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/targeted_recovery.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "../../../tests/observer_unit/controller_fact_coverage.rs"]
mod controller_fact_tests;

#[cfg(test)]
#[path = "../../../tests/observer_unit/v12_terminal_coverage.rs"]
mod v12_tests;

#[cfg(test)]
#[path = "../../../tests/observer_unit/planned_replay.rs"]
mod planned_tests;
