//! A run cannot complete while its exact delivery is awaiting a checkpoint.
use crate::{
    Result,
    completion_review::Key,
    observer_review::{self, state::Store},
};
use kah_run::verification::Receipt;
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug)]
pub enum Status {
    Pending,
    Passed,
    Failed(Value),
}
pub fn request(
    directory: &Path,
    worker: &str,
    key: &Key,
    revision: u64,
    receipt: &Receipt,
) -> Result<Status> {
    if receipt.commit != key.commit || receipt.command != key.command {
        return Err("RUN_CHECKPOINT_VERIFICATION stage=completion: checkpoint receipt is for different completion inputs".into());
    }
    let config = observer_review::control::config(worker)?
        .ok_or("RUN_CHECKPOINT_BINDING stage=completion")?;
    if !config.enabled {
        return Ok(Status::Passed);
    }
    if let Some(state) = Store::read(worker)? {
        if state.pending.is_some() {
            return Ok(Status::Pending);
        }
        if state
            .checkpoint
            .as_ref()
            .is_some_and(|c| same_delivery(&c["completion_key"], key))
            && let Some(passed) = observer_review::checkpoint::covers(&state, &key.commit, revision)
        {
            return Ok(if passed {
                Status::Passed
            } else {
                Status::Failed(state.checkpoint.unwrap())
            });
        }
    }
    let path = directory.join("completion-checkpoint-request.json");
    let previous: Option<Value> = observer_review::state::read(&path)?;
    if !previous
        .as_ref()
        .is_some_and(|p| same_delivery(&p["key"], key) && p["revision"] == revision)
    {
        observer_review::events::enqueue(
            worker,
            json!({"kind":"run_completion","completion_key":key,"intent_revision":revision,"verification":receipt,"actor":"kah controller; tests run on the agent's imported SHA, kah authored no commit"}),
        )?;
        crate::unattended_report::save(
            directory,
            "completion-checkpoint-request.json",
            &json!({"key":key,"revision":revision}),
        )?;
    }
    Ok(Status::Pending)
}

fn same_delivery(value: &Value, key: &Key) -> bool {
    serde_json::from_value::<Key>(value.clone()).is_ok_and(|saved| saved.same_delivery(key))
}
