use super::*;
use sha2::{Digest, Sha256};

pub fn store(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
    judgment: &Value,
) -> Result<Option<Value>> {
    if !eligible(request, report, assessment, proof) {
        return Ok(None);
    }
    let directory = root.join("questions");
    kah_core::run_store::ensure_private_directory(&directory)?;
    kah_core::run_store::ensure_private_directory(&root.join("sources"))?;
    let _lock = super::super::state::Store::delivery_at(&directory, true)?;
    for mut existing in super::super::questions::pending(root)?
        .into_iter()
        .filter(|q| q["kind"] == "checkpoint_approval" && q["worker"] == request.worker)
    {
        if existing["head"] == json!(request.head)
            && existing["intent_revision"] == request.intent["revision"]
        {
            return Ok(Some(existing));
        }
        existing["status"] = "obsolete".into();
        existing["obsolete_reason"] =
            "Checkpoint HEAD or intent changed; original question retained.".into();
        super::super::source_store::save(
            &directory,
            &format!("{}.json", existing["id"].as_str().unwrap()),
            &existing,
        )?;
    }
    let value = draft(root, request, report, assessment, proof, judgment)?;
    super::super::source_store::save(
        &directory,
        &format!("{}.json", value["id"].as_str().unwrap()),
        &value,
    )?;
    Ok(Some(value))
}
fn draft(
    root: &Path,
    request: &Request,
    report: &Report,
    assessment: &Value,
    proof: &DeliveryProof,
    judgment: &Value,
) -> Result<Value> {
    let name = report
        .findings
        .iter()
        .find_map(|f| f.checkpoint_scope.as_ref().map(|s| s.phase.clone()))
        .unwrap_or_else(|| "checkpoint".into());
    let ids = assessment["needs_thinker"].as_array().unwrap();
    let items = report
        .decision_units
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .filter(|r| ids.iter().any(|id| id == &r.id))
        .collect::<Vec<_>>();
    let names = items
        .iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let head = request.head.as_deref().unwrap();
    let summary = format!(
        "Gate {name} on {}: no violations; tests pass in fresh checkout; Jev couldn't confirm: {names}. Approve or reject?",
        &head[..8]
    );
    let identity = json!([
        request.worker,
        request.head,
        request.intent["revision"],
        request.id
    ]);
    let id = format!(
        "question-{:x}",
        Sha256::digest(serde_json::to_vec(&identity)?)
    );
    let mut value = json!({"kind":"checkpoint_approval","id":id,"worker":request.worker,"requirement":"gate_approval",
        "name":name,"request":request.id,"head":head,"intent_revision":request.intent["revision"],"summary":summary,
        "question":summary,"options":["Approve","Reject"],"status":"pending","delivery":"queued",
        "question_origin":"observer","at_ms":kah_observer::session::now_ms(),"thinker_items":items,"jev":judgment,"proof":proof,
        "assessment":assessment,"record":root.join("results").join(format!("{}.json",request.id)),
        "scope":"Only this checkpoint waits; unrelated authorized work continues",
        "authority_at_question":super::super::decisions::active_items(&request.intent),"goal_at_question":request.intent["goal"]});
    crate::privacy::scrub(&mut value);
    Ok(value)
}
pub fn message(question: &Value) -> Result<String> {
    Ok(format!(
        "{}\n[kah-builder] [kah-question] {} worker={}\nThinker's conclusions/reasoning/sources: {}\nJev answers: {}\nExact delivery receipts: {}\nFull record: {}\nOnly this checkpoint waits; unrelated authorized work continues. Relay an explicit user answer with: kah steer SESSION TEXT --question {} --answer approve|reject. Delegate text cannot approve.",
        question["summary"]
            .as_str()
            .unwrap_or("Gate decision required"),
        question["id"],
        question["worker"],
        serde_json::to_string(&question["thinker_items"])?,
        serde_json::to_string(&question["jev"])?,
        serde_json::to_string(&question["proof"])?,
        question["record"],
        question["id"]
            .as_str()
            .ok_or("REVIEW_QUESTION_ID stage=approval")?
    ))
}
