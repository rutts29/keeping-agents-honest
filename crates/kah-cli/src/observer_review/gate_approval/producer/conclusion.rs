//! Checked host results determine which items leave the uncertainty set.
use super::*;
use sha2::{Digest, Sha256};

pub(crate) fn finish(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
) -> Result<bool> {
    if request.trigger["verification_followup"]["stage"] != "checks_complete" {
        return Ok(false);
    }
    if assessment["semantic_resolution"]["status"] == "pending" {
        return Ok(true); // Durable SAME-observer context owns the next conclusion.
    }
    if super::super::contract_results::send_back(root, request, assessment)? {
        return Ok(true);
    }
    let round = start(root, request)?;
    if round.stage == Stage::Complete {
        complete_record(root, &round, &request.id)?;
        return Ok(true);
    }
    let directory = directory(root, &round);
    let job: Job = source_store::read(&directory.join("job.json"))?;
    let proof: DeliveryProof = source_store::read(&directory.join("checks.json"))?;
    if !proof.risk_receipts(&round.binding.head) || assessment["passed"] == false {
        // Failed checks/confirmed violations stay blocking; no risk admission.
        return Ok(true);
    }
    if !assessment["needs_user"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Ok(true);
    }
    let receipt = pointer(
        &round.checks_receipt(),
        &directory.join("checks.json"),
        &serde_json::to_value(&proof)?,
    )?;
    if !risk_compatible(report, assessment) {
        source_store::save(
            &directory,
            "finished.json",
            &json!({"status":"unconfirmed_observation","request":request.id,"assessment":assessment,"reason":"Final uncertainty is not exclusively positive requirements; no risk admission or repeated verification round"}),
        )?;
        return Ok(true);
    }
    let result_path = root.join("results").join(format!("{}.json", request.id));
    let result: Value = source_store::read(&result_path)?;
    let decision = pointer(&format!("{}-decision", round.id), &result_path, &result)?;
    let (verified, risks) = checked_items(&round, &job, report, assessment, &receipt, &decision)?;
    let round = advance(
        root,
        &round.binding,
        Step::Checks(CheckReceipt {
            receipt,
            proof,
            risks,
            verified,
        }),
    )?;
    complete_record(root, &round, &request.id)?;
    Ok(true)
}
fn complete_record(root: &Path, round: &VerificationRound, request: &str) -> Result<()> {
    let checks = round
        .checks
        .as_ref()
        .ok_or("REVIEW_FOLLOWUP_RECEIPTS stage=verification")?;
    source_store::save(
        &directory(root, round),
        "finished.json",
        &json!({
            "status":"completed","request":request,
            "verified":checks.verified.iter().map(|item|&item.requirement_id).collect::<Vec<_>>(),
            "remaining_risks":checks.risks.iter().map(|item|&item.requirement_id).collect::<Vec<_>>()
        }),
    )
}

fn items(
    round: &VerificationRound,
    job: &Job,
    report: &Report,
    assessment: &Value,
    receipt: &EvidencePointer,
    decision: &EvidencePointer,
) -> Result<(Vec<VerifiedRequirement>, Vec<UnverifiedRisk>)> {
    let units = report
        .decision_units
        .as_ref()
        .ok_or("REVIEW_FOLLOWUP_UNITS stage=verification")?;
    let unresolved = assessment["needs_thinker"]
        .as_array()
        .ok_or("REVIEW_FOLLOWUP_OUTCOME stage=verification")?;
    let mut verified = vec![];
    let mut risks = vec![];
    let ids = job.assessment["needs_thinker"]
        .as_array()
        .ok_or("REVIEW_FOLLOWUP_OUTCOME stage=verification")?
        .iter()
        .chain(unresolved)
        .map(|id| {
            id.as_str()
                .ok_or("REVIEW_FOLLOWUP_OUTCOME stage=verification")
        })
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    for id in ids {
        let (_current, is_new) = linked_requirement(id, job, report)?;
        if ["not_due", "deferred"].iter().any(|key| {
            assessment[*key]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|value| value == id))
        }) {
            // The final checked decision records an exemption/deferral, not satisfaction.
            continue;
        }
        if !unresolved.iter().any(|value| value == id) {
            verified.push(VerifiedRequirement {
                requirement_id: id.into(),
                evidence: round.evidence.clone().unwrap(),
                decision_receipt: decision.clone(),
            });
        } else {
            if super::super::semantic_resolution::needs_resolution(report, assessment, id) {
                // The durable same-observer resolution owns this item, not a user risk.
                continue;
            }
            let explanation = super::super::interactive::report_risk(report, id)?;
            risks.push(checked_risk(
                round,
                id,
                &explanation,
                receipt,
                units.version(),
                is_new,
            )?);
        }
    }
    Ok((verified, risks))
}
fn linked_requirement<'a>(
    id: &str,
    job: &Job,
    report: &'a Report,
) -> Result<(&'a kah_observer::decision_units::Requirement, bool)> {
    let current = report
        .decision_units
        .as_ref()
        .and_then(|u| u.requirements.iter().find(|r| r.id == id))
        .ok_or("REVIEW_FOLLOWUP_REQUIREMENT_MISSING stage=verification")?;
    let original = job
        .report
        .decision_units
        .as_ref()
        .and_then(|u| u.requirements.iter().find(|r| r.id == id));
    if !preserves(original, current) {
        return Err("REVIEW_FOLLOWUP_REQUIREMENT_CHANGED stage=verification: preserve semantic/factual kind and linked authority".into());
    }
    Ok((current, original.is_none()))
}
fn positive_requirement(requirement: &kah_observer::decision_units::Requirement) -> bool {
    match &requirement.judgment {
        Thinking::Semantic {
            decision: SemanticDecision::Satisfied,
            ..
        }
        | Thinking::AtomicFact { .. } => true,
        Thinking::ArtifactPreservation { fact, .. } => fact.equal(),
        _ => false,
    }
}
fn risk(
    round: &VerificationRound,
    id: &str,
    explanation: &kah_observer::review_evidence::VerificationRisk,
    receipt: &EvidencePointer,
) -> UnverifiedRisk {
    UnverifiedRisk {
        requirement_id: id.into(),
        unverified: explanation.unverified.clone(),
        acceptance_risk: explanation.acceptance_risk.clone(),
        evidence: round.evidence.clone().unwrap(),
        tried: vec![
            Attempt {
                what: "Read the existing source and check receipts with the persistent observer."
                    .into(),
                receipt: round.evidence.as_ref().unwrap()[0].receipt.clone(),
            },
            Attempt {
                what: match &round.builder.as_ref().unwrap().response {
                    BuilderOutcome::Evidence { .. } => {
                        "Asked the worker once for specific evidence and read its native reply."
                            .into()
                    }
                    BuilderOutcome::Unavailable { reason } => format!(
                        "Asked the worker once; delivery could not provide a reply: {reason}"
                    ),
                },
                receipt: round.builder_receipt(),
            },
            Attempt {
                what: format!(
                    "Read the exact-SHA verification inventory and check receipts for {}.",
                    round.binding.head
                ),
                receipt: receipt.receipt.clone(),
            },
        ],
    }
}
fn pointer(receipt: &str, record: &Path, value: &Value) -> Result<EvidencePointer> {
    Ok(EvidencePointer {
        receipt: receipt.into(),
        record: record.display().to_string(),
        digest: format!("sha256:{:x}", Sha256::digest(serde_json::to_vec(value)?)),
    })
}

pub(crate) fn guard(root: &Path, request: &Request, assessment: &mut Value) -> Result<()> {
    super::super::contract_results::apply(root, request, assessment)?;
    if request.trigger["verification_followup"]["stage"] != "checks_complete" {
        return Ok(());
    }
    let round = start(root, request)?;
    let proof: DeliveryProof = source_store::read(&directory(root, &round).join("checks.json"))?;
    if !proof.risk_receipts(&round.binding.head) {
        assessment["passed"] = false.into();
        assessment["state"] = "checkpoint_failed".into();
        let violations = assessment["violations"]
            .as_array_mut()
            .ok_or("REVIEW_FOLLOWUP_OUTCOME stage=verification")?;
        if !violations.iter().any(|id| id == "exact_sha_tests") {
            violations.push("exact_sha_tests".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/observer_unit/gate_producer.rs"]
mod tests;

fn preserves(
    original: Option<&kah_observer::decision_units::Requirement>,
    current: &kah_observer::decision_units::Requirement,
) -> bool {
    original.is_none_or(|original| {
        original.dimension == current.dimension
            && std::mem::discriminant(&original.judgment)
                == std::mem::discriminant(&current.judgment)
            && original
                .authority_items
                .iter()
                .all(|item| current.authority_items.contains(item))
    })
}

fn checked_items(
    round: &VerificationRound,
    job: &Job,
    report: &Report,
    assessment: &Value,
    receipt: &EvidencePointer,
    decision: &EvidencePointer,
) -> Result<(Vec<VerifiedRequirement>, Vec<UnverifiedRisk>)> {
    super::super::super::failure::validation(
        super::super::super::failure::Origin::ReportValidation,
        || items(round, job, report, assessment, receipt, decision),
    )
}

fn risk_compatible(report: &Report, assessment: &Value) -> bool {
    assessment["needs_thinker"].as_array().is_some_and(|ids| {
        ids.iter().all(|id| {
            report.decision_units.as_ref().is_some_and(|units| {
                units
                    .requirements
                    .iter()
                    .any(|r| id == &r.id && positive_requirement(r))
            })
        })
    })
}

fn contract_attempts(
    round: &VerificationRound,
    id: &str,
    receipt: &EvidencePointer,
) -> Result<super::super::contract_execution::ItemResult> {
    let path = Path::new(&receipt.record)
        .parent()
        .ok_or("REVIEW_CONTRACT_RECEIPTS stage=verification")?
        .join("item-results.json");
    let results: Vec<super::super::contract_execution::ItemResult> =
        super::super::super::failure::validation(
            super::super::super::failure::Origin::Host,
            || source_store::read(&path),
        )?;
    super::super::super::failure::validation(super::super::super::failure::Origin::Host, || {
        results
            .into_iter()
            .find(|r| r.binding == round.binding && r.requirement_id == id)
            .ok_or(
                "REVIEW_CONTRACT_RECEIPTS stage=verification: item-specific attempts required"
                    .into(),
            )
    })
}

fn checked_risk(
    round: &VerificationRound,
    id: &str,
    explanation: &kah_observer::review_evidence::VerificationRisk,
    receipt: &EvidencePointer,
    version: u32,
    new_requirement: bool,
) -> Result<UnverifiedRisk> {
    let mut value = risk(round, id, explanation, receipt);
    if version >= 12 && new_requirement {
        value.tried.push(Attempt {
            what: "This requirement was identified in the final review after the one verification round; no item-specific covering check was verified.".into(),
            receipt: receipt.receipt.clone(),
        });
        return Ok(value);
    }
    if version >= 10 {
        let result = contract_attempts(round, id, receipt)?;
        if !result.semantic_risk_admissible {
            return Err(format!(
                "REVIEW_CONTRACT_STATUS_RISK stage=verification: {id} is worker verification work"
            )
            .into());
        }
        value.tried = result.tried;
    }
    Ok(value)
}
