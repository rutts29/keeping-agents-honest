use super::*;
use kah_core::intent_history::{GateDecision, SourceRef};

pub fn resolve(
    root: &Path,
    source: &SourceRef,
    text: &str,
    head: &str,
    revision: u64,
    violations: &[String],
) -> Result<Option<Value>> {
    let Some(answer) = &source.question_answer else {
        return Ok(None);
    };
    source.validate()?;
    let directory = root.join("questions");
    let _lock = super::super::state::Store::delivery_at(&directory, true)?;
    let path = directory.join(format!("{}.json", answer.question));
    let mut question: Value =
        super::super::state::read(&path)?.ok_or("REVIEW_GATE_QUESTION_UNKNOWN stage=approval")?;
    if question["kind"] != "checkpoint_approval" || question["worker"] != source.session_private() {
        return Err("REVIEW_GATE_QUESTION_SCOPE stage=approval".into());
    }
    if question["status"] != "pending" {
        if question["resolution"]["source"]["record"] == source.record {
            return Ok(Some(question));
        }
        return Err("REVIEW_GATE_ANSWER_ALREADY_RESOLVED stage=approval".into());
    }
    if question["head"] != head || question["intent_revision"] != revision {
        return Err("REVIEW_GATE_ANSWER_STALE stage=approval: HEAD or intent changed; answer retained as user input".into());
    }
    if !violations.is_empty() {
        return Err(
            "REVIEW_GATE_CONFIRMED_VIOLATION stage=approval: confirmed violations still block"
                .into(),
        );
    }
    let proof: DeliveryProof = serde_json::from_value(question["proof"].clone())?;
    if !proof.passes(head) {
        return Err("REVIEW_GATE_DELIVERY_UNPROVEN stage=approval".into());
    }
    question["status"] = match answer.decision {
        GateDecision::Approve => "answered",
        GateDecision::Reject => "rejected",
    }
    .into();
    question["resolution"] = json!({"decision":answer.decision,"source":source,"verbatim":text,
        "head":head,"intent_revision":revision,"at_ms":kah_observer::session::now_ms(),
        "authority":"Explicit user answer; no model inference or delegate promotion"});
    super::super::source_store::save(
        &directory,
        path.file_name().unwrap().to_str().unwrap(),
        &question,
    )?;
    Ok(Some(question))
}
