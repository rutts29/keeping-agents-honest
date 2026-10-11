//! A code-known lack of recorded checks is a named acceptance risk, never a pass.
use super::*;
use sha2::{Digest, Sha256};
pub fn guard(request: &Request, assessment: &mut Value) -> Result<()> {
    let Some(proof) = super::super::captured(request)? else {
        return Ok(());
    };
    if request.trigger["interactive_gate_verification"].is_null() {
        return Ok(());
    }
    if proof
        .tests
        .iter()
        .any(|t| t.commit != proof.head || !t.success || t.exit_code != Some(0))
    {
        assessment["passed"] = false.into();
        assessment["state"] = "checkpoint_failed".into();
        let list = assessment["violations"]
            .as_array_mut()
            .ok_or("REVIEW_GATE_OUTCOME stage=verification")?;
        if !list.iter().any(|i| i == "exact_sha_tests") {
            list.push("exact_sha_tests".into());
        }
    } else if proof.recorded_checks_absent && assessment["passed"] != false {
        assessment["passed"] = Value::Null;
        assessment["state"] = "checkpoint_missing_recorded_checks".into();
        assessment["missing_recorded_checks"] = true.into();
    }
    Ok(())
}
pub(crate) fn missing_question(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
    judgment: &Value,
) -> Result<Option<Value>> {
    if request.trigger["interactive_gate_verification"].is_null()
        || assessment["passed"] != Value::Null
        || !assessment["violations"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !assessment["needs_user"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !request
            .head
            .as_deref()
            .is_some_and(|h| proof.risk_receipts(h))
    {
        return Ok(None);
    }
    let dir = root.join("questions");
    kah_core::run_store::ensure_private_directory(&dir)?;
    let _lock = Store::delivery_at(&dir, true)?;
    if let Some(q) = crate::observer_review::questions::pending(root)?
        .into_iter()
        .find(|q| {
            q["kind"] == "checkpoint_approval"
                && q["worker"] == request.worker
                && q["head"] == json!(request.head)
                && q["intent_revision"] == request.intent["revision"]
        })
    {
        return Ok(Some(q));
    }
    let q = draft(root, request, report, assessment, proof, judgment)?;
    source_store::save(&dir, &format!("{}.json", q["id"].as_str().unwrap()), &q)?;
    Ok(Some(q))
}
fn draft(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
    judgment: &Value,
) -> Result<Value> {
    let id = format!(
        "question-{:x}",
        Sha256::digest(serde_json::to_vec(&json!([
            request.worker,
            request.head,
            request.intent["revision"],
            request.id
        ]))?)
    );
    let name = report
        .findings
        .iter()
        .find_map(|f| f.checkpoint_scope.as_ref().map(|s| s.phase.clone()))
        .unwrap_or("checkpoint".into());
    let risks = risks(report, assessment)?;
    let names = risks
        .iter()
        .filter_map(|r| r["unverified"].as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let summary = format!(
        "Accept gate {name} on {} with these named unverified items: {names}, or send it back for more evidence?",
        request.head.as_deref().unwrap()
    );
    Ok(
        json!({"kind":"checkpoint_approval","id":id,"worker":request.worker,"requirement":"gate_approval","name":name,"request":request.id,"head":request.head,"intent_revision":request.intent["revision"],"summary":summary,"question":summary,"options":["Accept named unverified risks","Send back for evidence"],"status":"pending","delivery":"queued","question_origin":"controller","proof":proof,"assessment":assessment,"jev":judgment,"thinker_items":report.decision_units,"unverified_risks":risks,"record":root.join("questions").join(format!("{id}.json")),"authority_at_question":crate::observer_review::decisions::active_items(&request.intent),"goal_at_question":request.intent["goal"],"verification_round":{"source":request.trigger["interactive_gate_verification"],"checks_executed":0},"at_ms":kah_observer::session::now_ms()}),
    )
}
fn risks(report: &Report, assessment: &Value) -> Result<Vec<Value>> {
    let mut risks = vec![
        json!({"requirement_id":"recorded_checks","unverified":"No recorded project check was supplied for exact-SHA verification.","tried":[{"what":"Read checked native evidence; imported the exact gate SHA through fsck; created a private fresh checkout. No test command was invented or executed."}],"acceptance_risk":"Executable behavior has no objective test evidence on this gate SHA."}),
    ];
    for id in assessment["needs_thinker"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let risk=report.verification_risks.as_ref().and_then(|items|items.iter().find(|i|i.requirement_id==id)).ok_or("REVIEW_FOLLOWUP_RISK_MISSING stage=verification: explain each remaining gate item in plain words")?;
        risks.push(json!({"requirement_id":id,"unverified":risk.unverified,"tried":[{"what":"Read this item's checked evidence; no recorded project check was supplied to verify it on the exact SHA."}],"acceptance_risk":risk.acceptance_risk}));
    }
    Ok(risks)
}
