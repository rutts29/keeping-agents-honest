//! Plan ALL cache-miss contexts before waking the thinker; never cut content.
use super::*;
use crate::observer_review::{
    decisions::scoped,
    preparation,
    state::{self, Request, Store},
};
use kah_observer::judge::Questions;
use serde_json::Value;
use std::path::Path;
pub fn plan(request: &Request, input: &Value, questions: &Questions) -> Result<bool> {
    let current = preparation::active_decision(&request.worker, &request.id)?;
    let root = Store::directory(&request.worker)?;
    if let Some(batch) = load(&request.worker, &current)? {
        if pending(&request.worker, &batch)? && !accept(&request.worker, &batch)? {
            return Ok(true);
        }
    } else if preparation::pending(&request.worker, &current)?
        && !preparation::accept(&request.worker, &current)?
        && single_inflight(request, &root, &current)?
    {
        return Ok(true); // The accepted native single selection finishes first.
    }
    let calibration = preparation::calibration(&request.worker)?;
    let mut jobs = vec![];
    for (key, question) in questions {
        let mut state = scoped::report_context(input, key)?;
        crate::privacy::scrub(&mut state);
        let questions = Questions::from([(key.clone(), question.clone())]);
        let local = scoped::request_for(request, &state, &questions)?;
        if let Some(job) = wanted(&root, &local, &state, &questions, calibration.as_ref())? {
            jobs.push(job);
        }
    }
    if jobs.is_empty() {
        return Ok(false);
    }
    stage(&request.worker, &request.id, &jobs)?;
    Ok(true)
}
fn wanted(
    root: &Path,
    request: &Request,
    input: &Value,
    questions: &Questions,
    calibration: Option<&kah_observer::context_budget::Calibration>,
) -> Result<Option<Job>> {
    if root
        .join("decisions")
        .join(format!("{}.json", request.id))
        .is_file()
        || root.join(format!("prepared-{}.json", request.id)).is_file()
    {
        return Ok(None);
    }
    if let Some(job) = state::read::<Job>(&root.join(format!("preparation-{}.json", request.id)))? {
        if !preparation::matches_decision(&job, input, questions)? {
            return Err("PREPARATION_BATCH_CHANGED stage=context_preparation".into());
        }
        if root.join(format!("selection-{}.json", request.id)).exists() {
            return Ok(None);
        }
        return Ok(Some(job));
    }
    let Some(plan) = calibration
        .and_then(|c| c.plan(input, questions))
        .filter(|p| p.prepare)
    else {
        return Ok(None);
    };
    Ok(Some(preparation::job(
        request,
        input,
        questions,
        &serde_json::to_string(&plan)?,
    )))
}

#[cfg(test)]
#[path = "../../../../tests/observer_unit/preparation_batch_planning.rs"]
mod tests;

fn single_inflight(request: &Request, root: &Path, decision: &str) -> Result<bool> {
    if root
        .join(format!("preparation-woken-{decision}.json"))
        .exists()
    {
        return Ok(true);
    }
    let Some(state) = Store::read(&request.worker)? else {
        return Ok(false);
    };
    if state
        .pending
        .as_ref()
        .is_none_or(|pending| pending.id != request.id)
    {
        return Ok(false);
    }
    crate::observer_review::delivery::accepted(&state)
}
