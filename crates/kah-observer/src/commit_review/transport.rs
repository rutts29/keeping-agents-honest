//! Same production concurrent transport for live and credential-free replay.
use super::*;
use crate::judge::{
    Answer,
    batch::{Batch, Mode, Transport, WireRequest},
};
pub fn evaluate(
    worker: &str,
    head: &str,
    policy: &DoubtLine,
    batch: &Batch,
    requests: &[WireRequest],
    transport: &impl Transport,
) -> Result<Review, String> {
    if requests.is_empty() {
        let mut record = classify(worker, head, &[], policy)?;
        record
            .gaps
            .push(serde_json::json!({"route":"direct_observer_review",
            "reason":"no_literal_rules","full_intent_retained":true}));
        return Ok(record);
    }
    for request in requests {
        let body: Value = serde_json::from_str(&request.body)
            .map_err(|_| "COMMIT_BODY_JSON stage=commit_review")?;
        if body["questions"]
            .as_object()
            .is_none_or(|questions| questions.values().any(|q| q["type"] != "noul"))
        {
            return Err("COMMIT_LITERAL_ONLY stage=commit_review".into());
        }
    }
    let outcome = batch
        .run(requests, transport, Mode::Concurrent)
        .map_err(|e| e.to_string())?;
    let mut answers = vec![];
    let mut gaps = vec![];
    for receipt in outcome.responses {
        match receipt.judgment(receipt.elapsed_ms) {
            Ok(judgment) => for (question, answer) in judgment.answers {
                let Answer::Noul { noul } = answer else {
                    return Err("COMMIT_LITERAL_ONLY stage=commit_review".into());
                };
                answers.push(serde_json::json!({"request":receipt.request.id,"question":question,"noul":noul}));
            },
            Err(error) if error.code == "JEV_CONTEXT_LIMIT" => gaps.push(serde_json::json!({
                "request":receipt.request.id,"error":error.to_string(),"body_digest":crate::context_selection::digest(&serde_json::from_str::<Value>(&receipt.request.body).map_err(|_|"COMMIT_BODY_JSON stage=commit_review")?),
                "route":"direct_observer_review","known_oversized":true})),
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut record = classify(worker, head, &answers, policy)?;
    record.gaps = gaps;
    Ok(record)
}
