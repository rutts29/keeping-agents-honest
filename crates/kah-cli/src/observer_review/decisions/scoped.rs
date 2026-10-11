//! Independent complete decisions, cached by exact input for preparation retries.
use super::{Request, Value};
use crate::Result;
use kah_observer::{
    decision_units::context,
    judge::{Judgment, Metrics, Questions},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Decided {
    judgment: Judgment,
    #[serde(rename = "prepared_state")]
    prepared: Value,
    archive: Value,
}

pub fn evaluate(
    home: &Path,
    request: &Request,
    input: &Value,
    questions: &Questions,
) -> Result<(Judgment, Value, Value)> {
    plan_context(request, input, questions)?;
    super::scoped_batch::run(home, request, input, questions)
}

fn plan_context(request: &Request, input: &Value, questions: &Questions) -> Result<()> {
    super::super::consume::outcome::stage(&request.worker, &request.id, "batch_plan_started")?;
    let needs_preparation = super::super::preparation::batch::plan(request, input, questions)?;
    super::super::consume::outcome::stage(
        &request.worker,
        &request.id,
        if needs_preparation {
            "batch_preparation_handoff"
        } else {
            "batch_plan_complete"
        },
    )?;
    Ok(())
}
pub(super) fn decision_stage(request: &Request, key: &str, stage: &str) -> Result<()> {
    super::super::consume::outcome::stage(
        &request.worker,
        &request.id,
        &format!("decision_{stage}:{key}"),
    )
}

pub(crate) fn report_context(input: &Value, key: &str) -> Result<Value> {
    super::super::failure::validation(super::super::failure::Origin::ReportValidation, || {
        context::decision(input, key).map_err(Into::into)
    })
}

pub(crate) type Evaluation = (Judgment, Value, Value);
pub(super) fn load_decision(request: &Request) -> Result<Option<Evaluation>> {
    let path = super::super::state::Store::directory(&request.worker)?
        .join("decisions")
        .join(format!("{}.json", request.id));
    let saved: Option<Decided> = super::super::state::read(&path)?;
    Ok(saved.map(|s| (s.judgment, s.prepared, s.archive)))
}
/// Production and recorded transport share the exact-input durable decision cache.
pub(crate) fn decide_with(
    original: &Request,
    input: &Value,
    questions: &Questions,
    evaluate: impl FnOnce(&Request) -> Result<Evaluation>,
) -> Result<(Evaluation, bool)> {
    let request = request_for(original, input, questions)?;
    let root = super::super::state::Store::directory(&request.worker)?.join("decisions");
    kah_core::run_store::ensure_private_directory(&root)?;
    let name = format!("{}.json", request.id);
    if let Some(saved) = super::super::state::read::<Decided>(&root.join(&name))? {
        return Ok(((saved.judgment, saved.prepared, saved.archive), true));
    }
    let (judgment, prepared, archive) = evaluate(&request)?;
    let decided = Decided {
        judgment,
        prepared,
        archive,
    };
    super::super::source_store::save(&root, &name, &serde_json::to_value(&decided)?)?;
    Ok(((decided.judgment, decided.prepared, decided.archive), false))
}

pub(crate) fn merge(combined: &mut Judgment, part: &Judgment, reused: bool) -> Result<()> {
    if combined.backend == "not_requested" {
        combined.backend = part.backend.clone();
        combined.model = part.model.clone();
        combined.fallback_reason = part.fallback_reason.clone();
    } else if combined.backend != part.backend || combined.model != part.model {
        return Err(
            "REVIEW_DECISION_BACKEND stage=decision: independent decisions used different judges"
                .into(),
        );
    }
    for (key, answer) in &part.answers {
        if combined
            .answers
            .insert(key.clone(), answer.clone())
            .is_some()
        {
            return Err("REVIEW_DECISION_DUPLICATE stage=decision".into());
        }
    }
    if !reused {
        metrics(&mut combined.metrics, &part.metrics);
    }
    Ok(())
}

fn metrics(total: &mut Metrics, part: &Metrics) {
    total.calls += part.calls;
    total.failed_calls += part.failed_calls;
    total.input_tokens += part.input_tokens;
    total.cached_input_tokens += part.cached_input_tokens;
    total.output_tokens += part.output_tokens;
    total.elapsed_ms += part.elapsed_ms;
    if let Some(bytes) = part.input_utf8_bytes {
        *total.input_utf8_bytes.get_or_insert(0) += bytes;
    }
    if let Some(cost) = part.estimated_cost_usd {
        *total.estimated_cost_usd.get_or_insert(0.0) += cost;
    }
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/scoped_metrics.rs"]
mod tests;

pub(crate) fn request_for(
    original: &Request,
    input: &Value,
    questions: &Questions,
) -> Result<Request> {
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!({
        "state":input,"questions":questions}))?)
    );
    Ok(Request {
        id: format!("{}-v8-{digest}", original.id),
        worker: original.worker.clone(),
        purpose: original.purpose.clone(),
        turn: original.turn.clone(),
        head: original.head.clone(),
        intent: original.intent.clone(),
        trigger: json!({"parent_review":original.id}),
        documents: vec![],
        created_ms: original.created_ms,
    })
}
