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
    if assessment["passed"] != Value::Null {
        return Ok(None);
    }
    let Some(followup) = followup::completed(root, request)? else {
        return Ok(None);
    };
    if proof.recorded_checks_absent {
        return super::interactive::missing_question(
            root, request, report, assessment, proof, judgment,
        );
    }
    if !eligible_after_followup(request, report, assessment, proof, &followup) {
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
    let value = draft(root, request, report, assessment, judgment, &followup)?;
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
    judgment: &Value,
    followup: &followup::CompletedFollowup,
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
    let head = request.head.as_deref().unwrap();
    let proof = &followup.round().checks.as_ref().unwrap().proof;
    let names = followup
        .risks()
        .iter()
        .map(|risk| risk.unverified.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let summary = format!(
        "Accept gate {name} on {head} with these named unverified items: {names}, or send it back for more evidence?"
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
        "question":summary,"options":["Accept named unverified risks","Send back for evidence"],"status":"pending","delivery":"queued",
        "question_origin":"observer","at_ms":kah_observer::session::now_ms(),"thinker_items":items,"jev":judgment,"proof":proof,
        "assessment":assessment,"followup":followup.round(),"unverified_risks":followup.risks(),"record":root.join("questions").join(format!("{id}.json")),"review_record":root.join("results").join(format!("{}.json",request.id)),
        "scope":"Only this checkpoint waits; unrelated authorized work continues",
        "authority_at_question":super::super::decisions::active_items(&request.intent),"goal_at_question":request.intent["goal"]});
    crate::privacy::scrub(&mut value);
    Ok(value)
}
pub fn message(question: &Value) -> Result<String> {
    let risks = question["unverified_risks"]
        .as_array()
        .ok_or("REVIEW_GATE_RISKS stage=approval")?
        .iter()
        .map(|risk| {
            let tried = risk["tried"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["what"].as_str())
                .collect::<Vec<_>>()
                .join("; ");
            format!(
                "{} (reference: {})\nTried: {tried}\nRisk if accepted: {}",
                risk["unverified"].as_str().unwrap_or(""),
                risk["requirement_id"].as_str().unwrap_or(""),
                risk["acceptance_risk"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(format!(
        "[kah-builder] {}\n{risks}\n[kah-question] {} worker={}\nFull record for {}: {}\nOnly this checkpoint waits; unrelated authorized work continues. An explicit approve answer accepts the named unverified risks; reject sends back for evidence. Relay the user decision with: kah steer SESSION TEXT --question {} --answer approve|reject. Delegate text cannot approve.",
        question["summary"]
            .as_str()
            .unwrap_or("Gate decision required"),
        question["id"]
            .as_str()
            .ok_or("REVIEW_QUESTION_ID stage=approval")?,
        question["worker"],
        question["id"]
            .as_str()
            .ok_or("REVIEW_QUESTION_ID stage=approval")?,
        question["record"],
        question["id"]
            .as_str()
            .ok_or("REVIEW_QUESTION_ID stage=approval")?
    ))
}
