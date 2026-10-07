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
        if let Some(checkpoint) = &state.checkpoint
            && approval_delivery(directory, checkpoint, key)?
            && observer_review::gate_approval::approved_authority(&state, revision)?
        {
            return Ok(Status::Passed);
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

fn approval_delivery(directory: &Path, checkpoint: &Value, key: &Key) -> Result<bool> {
    if checkpoint["state"] != "checkpoint_user_approved" || checkpoint["head"] != key.commit {
        return Ok(false);
    }
    let saved: Key = serde_json::from_value(checkpoint["completion_key"].clone())?;
    if saved.command != key.command || saved.online != key.online {
        return Ok(false);
    }
    let record: kah_run::store::Record =
        observer_review::state::read(&directory.join("state.json"))?
            .ok_or("RUN_STATE_MISSING stage=completion")?;
    let added = record
        .control
        .inputs()
        .iter()
        .filter(|i| i.sequence > saved.user_sequence)
        .collect::<Vec<_>>();
    Ok(key.user_sequence == saved.user_sequence
        || (added.len() == 1
            && added[0].sequence == key.user_sequence
            && added[0].text == checkpoint["user_answer"]["verbatim"]))
}

#[cfg(test)]
#[path = "../tests/observer_unit/approval_delivery.rs"]
mod tests;
