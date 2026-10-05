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
    let mut combined = Judgment {
        backend: "not_requested".into(),
        model: "none".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    let mut states = serde_json::Map::new();
    let mut archives = serde_json::Map::new();
    for (key, question) in questions {
        let mut state = context::decision(input, key)?;
        crate::privacy::scrub(&mut state);
        let questions = Questions::from([(key.clone(), question.clone())]);
        let (decided, reused) = decide(home, request, &state, &questions)?;
        merge(&mut combined, &decided.judgment, reused)?;
        states.insert(key.clone(), decided.prepared);
        archives.insert(
            key.clone(),
            json!({"capture":decided.archive,"reused":reused}),
        );
    }
    Ok((
        combined,
        json!({"independent_decisions":states}),
        Value::Object(archives),
    ))
}

fn decide(
    home: &Path,
    original: &Request,
    input: &Value,
    questions: &Questions,
) -> Result<(Decided, bool)> {
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!({
        "state":input,"questions":questions}))?)
    );
    let mut request = original.clone();
    request.id = format!("{}-v8-{digest}", original.id);
    let root = super::super::state::Store::directory(&request.worker)?.join("decisions");
    kah_core::run_store::ensure_private_directory(&root)?;
    let name = format!("{}.json", request.id);
    if let Some(saved) = super::super::state::read(&root.join(&name))? {
        return Ok((saved, true));
    }
    if super::super::preparation::pending(&request.worker, &request.id)? {
        super::super::preparation::accept(&request.worker, &request.id)?;
    }
    let (judgment, prepared) =
        super::super::preparation::evaluate(home, &request, input, questions)?;
    let archive = super::capture(home, &request, &judgment, &prepared, questions, 8)?;
    let decided = Decided {
        judgment,
        prepared,
        archive: serde_json::to_value(archive)?,
    };
    super::super::source_store::save(&root, &name, &serde_json::to_value(&decided)?)?;
    Ok((decided, false))
}

fn merge(combined: &mut Judgment, part: &Judgment, reused: bool) -> Result<()> {
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
