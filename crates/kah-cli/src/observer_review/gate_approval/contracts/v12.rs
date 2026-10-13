//! Executable coverage is a thinker assertion validated against a bound receipt.
use super::*;
use kah_observer::{decision_units::Requirement, verification_contract::reasoned};

pub(super) fn prepare(
    request: &Request,
    report: &Report,
    input: &mut Value,
    questions: &mut Questions,
    proof: Option<&DeliveryProof>,
) -> Result<()> {
    let Some(items) = &report.verification_contracts else {
        return Ok(());
    };
    let units = report
        .decision_units
        .as_ref()
        .ok_or("REVIEW_CONTRACT_UNITS stage=verification")?;
    let verified = super::super::super::decisions::verify_evidence(request, report)?
        .ok_or("REVIEW_CONTRACT_UNITS stage=verification")?;
    let mut seen = BTreeSet::new();
    input["verification_contracts"] = json!({});
    for item in items {
        if !seen.insert(&item.requirement_id)
            || item.claim.trim().is_empty()
            || item.coverage_reasoning.trim().is_empty()
        {
            return Err("REVIEW_CONTRACT_ITEM stage=verification: duplicate or empty item".into());
        }
        let requirement = units
            .requirements
            .iter()
            .find(|r| r.id == item.requirement_id)
            .ok_or("REVIEW_CONTRACT_ITEM stage=verification: unknown requirement")?;
        let checked = verified["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == item.requirement_id)
            .unwrap();
        if item.evidence_citations.is_empty() {
            input["verification_contracts"][&item.requirement_id] =
                json!({"gap":{"code":"REVIEW_CONTRACT_UNCITED","stage":"coverage"}});
            continue;
        }
        selected(item, checked)?;
        if matches!(
            requirement.judgment,
            Thinking::AtomicFact { .. } | Thinking::ControllerFact { .. }
        ) {
            if matches!(item.method, Method::SemanticRisk { .. }) {
                return Err("REVIEW_CONTRACT_LITERAL_RISK stage=verification".into());
            }
            continue;
        }
        one(request, item, requirement, checked, proof, input, questions)?;
    }
    Ok(())
}
fn one(
    request: &Request,
    item: &Item,
    requirement: &Requirement,
    checked: &Value,
    proof: Option<&DeliveryProof>,
    input: &mut Value,
    questions: &mut Questions,
) -> Result<()> {
    if matches!(item.method, Method::ControllerReceipt { .. }) {
        return controller(request, item, requirement, checked, proof, input, questions);
    }
    let commands = proof
        .map(|p| {
            p.tests
                .iter()
                .map(|t| t.command.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if matches!(item.method, Method::SemanticRisk { .. }) {
        return super::add_questions(
            request,
            item,
            checked,
            &selected(item, checked)?,
            &commands,
            input,
            questions,
        );
    }
    let Some(receipt) = receipt(request, item, proof)? else {
        return Ok(());
    };
    if receipt.commit != request.head.as_deref().unwrap_or("")
        || receipt.exit_code != Some(0)
        || !receipt.success
    {
        return Ok(());
    }
    let (state, local) = reasoned::prepare(
        item,
        requirement,
        checked,
        input,
        &receipt.command,
        &serde_json::to_value(&receipt)?,
    )?;
    add(item, input, questions, state, local);
    Ok(())
}
fn controller(
    request: &Request,
    item: &Item,
    requirement: &Requirement,
    checked: &Value,
    proof: Option<&DeliveryProof>,
    input: &mut Value,
    questions: &mut Questions,
) -> Result<()> {
    let Method::ControllerReceipt { facts } = &item.method else {
        return Err("REVIEW_CONTRACT_METHOD stage=coverage".into());
    };
    if !proof.is_some_and(|p| super::super::contract_facts::verified(request, p, facts)) {
        return Ok(());
    }
    let (state, local) = reasoned::controller(
        item,
        requirement,
        checked,
        input,
        &json!({"head":request.head,"facts":facts,"code_verified":true}),
    )?;
    add(item, input, questions, state, local);
    Ok(())
}
fn add(item: &Item, input: &mut Value, questions: &mut Questions, state: Value, local: Questions) {
    input["verification_contracts"][&item.requirement_id] = state;
    for (key, question) in local {
        questions.insert(format!("{key}.{}", item.requirement_id), question);
    }
}
fn receipt(
    request: &Request,
    item: &Item,
    proof: Option<&DeliveryProof>,
) -> Result<Option<TestReceipt>> {
    match &item.method {
        Method::ExistingCheck { test_index } => proof
            .map(|p| {
                p.tests
                    .get(*test_index)
                    .cloned()
                    .ok_or("REVIEW_CONTRACT_RECEIPT stage=verification: unknown check".into())
            })
            .transpose(),
        Method::TargetedCheck { command } => {
            request.trigger["verification_followup"]["item_results"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| {
                    r["requirement_id"] == item.requirement_id
                        && r["check"]["command"] == command.as_str()
                })
                .map(|r| serde_json::from_value(r["check"].clone()).map_err(Into::into))
                .transpose()
        }
        _ => Ok(None),
    }
}
