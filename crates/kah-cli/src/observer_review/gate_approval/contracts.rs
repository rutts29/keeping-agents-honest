//! Per-item coverage shares the existing thinker/Jev evidence path.
use super::*;
use kah_observer::{
    judge::{Judgment, Questions},
    verification_contract::{Item, Method},
};
use std::collections::BTreeSet;

pub(crate) fn prepare(
    request: &Request,
    report: &Report,
    input: &mut Value,
    questions: &mut Questions,
) -> Result<()> {
    let Some(items) = &report.verification_contracts else {
        return Ok(());
    };
    let units = report
        .decision_units
        .as_ref()
        .ok_or("REVIEW_CONTRACT_UNITS stage=verification")?;
    let verified = super::super::decisions::verify_evidence(request, report)?
        .ok_or("REVIEW_CONTRACT_UNITS stage=verification")?;
    let commands = captured(request)?
        .map(|p| p.tests.into_iter().map(|t| t.command).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    input["verification_contracts"] = json!({});
    for item in items {
        let requirement = units
            .requirements
            .iter()
            .find(|r| r.id == item.requirement_id)
            .ok_or("REVIEW_CONTRACT_ITEM stage=verification: unknown requirement")?;
        if !seen.insert(&item.requirement_id)
            || item.claim.trim().is_empty()
            || item.coverage_reasoning.trim().is_empty()
        {
            return Err("REVIEW_CONTRACT_ITEM stage=verification: duplicate or empty item".into());
        }
        let value = verified["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == item.requirement_id)
            .unwrap();
        let evidence = selected(item, value)?;
        if matches!(requirement.judgment, Thinking::AtomicFact { .. })
            && matches!(item.method, Method::SemanticRisk { .. })
        {
            return Err(
                "REVIEW_CONTRACT_LITERAL_RISK stage=verification: receipt facts are code work"
                    .into(),
            );
        }
        if matches!(requirement.judgment, Thinking::AtomicFact { .. }) {
            continue;
        }
        add_questions(request, item, value, &evidence, &commands, input, questions)?;
    }
    Ok(())
}
fn add_questions(
    request: &Request,
    item: &Item,
    requirement: &Value,
    evidence: &[Value],
    commands: &[String],
    input: &mut Value,
    questions: &mut Questions,
) -> Result<()> {
    let command = item.command(commands)?.map(str::to_owned).or_else(|| match &item.method {
        Method::ControllerReceipt { facts } => Some(format!("kah compares these trusted controller receipt properties against the request HEAD: {}", json!(facts))),
        Method::SemanticRisk { .. } => Some(String::new()),
        _ => None,
    });
    if let Some(command) = command {
        let mut context = item.state(
            &command,
            &request.intent,
            evidence,
            request.head.as_deref().unwrap_or(""),
        );
        context["coverage"]["proposed_claim"] = json!(item.claim);
        context["coverage"]["item"]["claim"] = requirement["name"].clone();
        context["coverage"]["requirement"] = json!({"id":requirement["id"],"name":requirement["name"],"judgment":requirement["judgment"],"authority_items":requirement["authority_items"]});
        input["verification_contracts"][&item.requirement_id] = context;
        for (key, q) in item.questions() {
            questions.insert(format!("{key}.{}", item.requirement_id), q);
        }
    }
    Ok(())
}
pub(crate) fn outcome(request: &Request, report: &Report, judgment: &Judgment) -> Result<Value> {
    let proof = captured(request)?;
    Ok(json!(
        report
            .verification_contracts
            .iter()
            .flatten()
            .map(|i| (
                &i.requirement_id,
                json!({"covered":covered(i,judgment) && match &i.method {Method::ControllerReceipt{facts}=>proof.as_ref().is_some_and(|p|super::contract_facts::verified(request,p,facts)),_=>true},"semantic_risk_admissible":matches!(i.method,Method::SemanticRisk{..}) && kah_observer::decision_units::confirmed(judgment,&format!("risk_admission.{}",i.requirement_id),"supported"),"method":i.method,"claim":i.claim})
            ))
            .collect::<std::collections::BTreeMap<_, _>>()
    ))
}
fn covered(item: &Item, judgment: &Judgment) -> bool {
    let mut local = judgment.clone();
    for key in ["coverage_subject", "coverage_assertion"] {
        if let Some(answer) = judgment
            .answers
            .get(&format!("{key}.{}", item.requirement_id))
        {
            local.answers.insert(key.into(), answer.clone());
        }
    }
    item.covered(&local)
}

fn selected(item: &Item, value: &Value) -> Result<Vec<Value>> {
    let evidence = item
        .evidence_citations
        .iter()
        .map(|i| {
            value["citations"]
                .get(*i)
                .cloned()
                .ok_or("REVIEW_CONTRACT_CITATION stage=verification".into())
        })
        .collect::<Result<Vec<_>>>()?;
    if evidence.is_empty() {
        return Err("REVIEW_CONTRACT_CITATION stage=verification".into());
    }

    Ok(evidence)
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/contract_claim_binding.rs"]
mod claim_tests;
