//! Preparation and assembly stay ordered; only independent ready requests overlap.
use super::scoped::*;
use super::{Request, Value};
use crate::{Result, observer_review::preparation};
use kah_observer::judge::{Judgment, Questions};
use std::path::Path;
struct Work {
    key: String,
    local: Request,
    state: Value,
    questions: Questions,
    ready: Option<preparation::ready::Ready>,
    saved: Option<(Judgment, Value, Value)>,
    error: Option<Box<dyn std::error::Error>>,
}
pub(super) fn run(
    home: &Path,
    request: &Request,
    input: &Value,
    questions: &Questions,
) -> Result<(Judgment, Value, Value)> {
    let mut work = prepare(request, input, questions)?;
    let ready = work
        .iter()
        .filter_map(|w| w.ready.as_ref())
        .filter(|r| r.cached.is_none())
        .cloned()
        .collect::<Vec<_>>();
    let receipts = preparation::send::send(home, &request.worker, &request.id, &ready)?;
    let mut responses = receipts
        .into_iter()
        .map(|r| (r.request.id.clone(), r))
        .collect::<std::collections::BTreeMap<_, _>>();
    for item in &mut work {
        let Some(ready) = item.ready.take() else {
            continue;
        };
        let (result, reused) = if let Some(judgment) = ready.cached.clone() {
            (Ok((judgment, ready.input.clone())), false)
        } else {
            let response = responses
                .remove(&item.local.id)
                .ok_or("JEV_BATCH_BINDING stage=decision")?;
            (
                preparation::ready::finish(&ready, response.judgment(response.elapsed_ms)),
                response.reused,
            )
        };
        match result {
            Ok((judgment, prepared)) => {
                let archive =
                    super::capture(home, &item.local, &judgment, &prepared, &ready.questions, 8)?;
                let ((judgment, prepared, archive), cached) =
                    decide_with(request, &item.state, &item.questions, |_| {
                        Ok((judgment, prepared, serde_json::to_value(&archive)?))
                    })?;
                item.saved = Some((
                    judgment,
                    prepared,
                    serde_json::json!({"capture":archive,"reused":reused||cached}),
                ));
            }
            Err(error) => item.error = Some(error),
        }
    }
    if work.iter().any(|w| {
        w.error
            .as_ref()
            .is_some_and(|e| e.to_string().starts_with("JEV_PREPARATION_PENDING"))
    }) {
        // New provider-budget discoveries become one deterministic native batch.
        preparation::batch::plan(request, input, questions)?;
    }
    assemble(request, work)
}
fn prepare(request: &Request, input: &Value, questions: &Questions) -> Result<Vec<Work>> {
    let calibration = preparation::calibration(&request.worker)?;
    let mut work = vec![];
    for (key, question) in questions {
        decision_stage(request, key, "started")?;
        let mut state =
            crate::observer_review::decision_deferral_scope::decision(request, input, key)?;
        crate::privacy::scrub(&mut state);
        let questions = Questions::from([(key.clone(), question.clone())]);
        let local = request_for(request, &state, &questions)?;
        let mut item = Work {
            key: key.clone(),
            local: local.clone(),
            state: state.clone(),
            questions: questions.clone(),
            ready: None,
            saved: None,
            error: None,
        };
        if let Some((judgment, prepared, archive)) = load_decision(&local)? {
            item.saved = Some((
                judgment,
                prepared,
                serde_json::json!({"capture":archive,"reused":true}),
            ));
        } else {
            if preparation::pending(&local.worker, &local.id)? {
                preparation::accept(&local.worker, &local.id)?;
            }
            match preparation::ready::prepare(&local, &state, &questions, calibration.as_ref()) {
                Ok(ready) => item.ready = Some(ready),
                Err(error) => item.error = Some(error),
            }
        }
        work.push(item);
    }
    Ok(work)
}
fn assemble(request: &Request, work: Vec<Work>) -> Result<(Judgment, Value, Value)> {
    let mut combined = Judgment {
        backend: "not_requested".into(),
        model: "none".into(),
        answers: Default::default(),
        metrics: Default::default(),
        fallback_reason: None,
    };
    let mut states = serde_json::Map::new();
    let mut archives = serde_json::Map::new();
    let mut pending: Option<String> = None;
    for item in work {
        if let Some(error) = item.error {
            let message = error.to_string();
            if !message.starts_with("JEV_PREPARATION_PENDING")
                && !message.starts_with("PREPARATION_CONTEXT_UNCONFIRMED")
            {
                return Err(error);
            }
            if pending
                .as_ref()
                .is_none_or(|p| !p.starts_with("PREPARATION_CONTEXT_UNCONFIRMED"))
            {
                pending = Some(format!("{error}; decision={}", item.key));
            }
            continue;
        }
        let (judgment, state, archive) = item.saved.ok_or("JEV_BATCH_BINDING stage=decision")?;
        merge(&mut combined, &judgment, archive["reused"] == true)?;
        decision_stage(request, &item.key, "finished")?;
        states.insert(item.key.clone(), state);
        archives.insert(item.key, archive);
    }
    let terminal = pending
        .as_ref()
        .filter(|e| e.starts_with("PREPARATION_CONTEXT_UNCONFIRMED"))
        .cloned();
    if terminal.is_none()
        && let Some(error) = pending
    {
        return Err(error.into());
    }
    Ok((
        combined,
        serde_json::json!({"independent_decisions":states,"unconfirmed_context":terminal}),
        Value::Object(archives),
    ))
}
