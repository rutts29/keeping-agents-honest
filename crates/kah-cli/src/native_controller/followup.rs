//! Idle follow-up progress is primary native evidence, independent of tool checks.
use super::*;
use crate::observer_review::{
    gate_approval::followup::{Stage, VerificationRound},
    source_store,
};
use std::{collections::BTreeSet, fs};
mod claude;
mod codex;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Progress {
    round: String,
    stage: &'static str,
    receipt: String,
    reply: String,
    completed: String,
}

#[derive(Default)]
pub(super) struct Tracker {
    seen: BTreeSet<Progress>,
}
pub(super) fn poll(
    state: &session::State,
    history: &mut History,
    tracker: &mut Tracker,
    mut schedule: impl FnMut() -> Result<()>,
) -> Result<()> {
    for round in pending(&state.binding.session)? {
        let receipt = round.builder_receipt();
        let completed = match history {
            History::Claude { path, session, .. } => claude::progress(path, session, &receipt)?,
            History::Codex {
                client, session, ..
            } => codex::progress(client, session, &receipt)?,
        };
        let Some((reply, completed)) = completed else {
            continue;
        };
        let progress = Progress {
            round: round.id,
            receipt,
            reply,
            completed,
            stage: if round.stage == Stage::Builder {
                "builder"
            } else {
                "checks"
            },
        };
        if !tracker.seen.contains(&progress) {
            schedule()?;
            tracker.seen.insert(progress);
        }
    }
    Ok(())
}

fn pending(worker: &str) -> Result<Vec<VerificationRound>> {
    let root = crate::observer_review::state::Store::directory(worker)?.join("gate-followups");
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut rounds = vec![];
    for entry in fs::read_dir(&root)
        .map_err(|e| format!("RUN_FOLLOWUP_STORE_IO stage=followup_progress: {e}"))?
    {
        let path = entry
            .map_err(|e| format!("RUN_FOLLOWUP_STORE_IO stage=followup_progress: {e}"))?
            .path();
        if !path.is_file()
            || path.extension().is_none_or(|e| e != "json")
            || path
                .file_name()
                .is_none_or(|n| !n.to_string_lossy().starts_with("followup-"))
        {
            continue;
        }
        let round: VerificationRound = source_store::read(&path)
            .map_err(|e| format!("RUN_FOLLOWUP_STATE stage=followup_progress: {e}"))?;
        if round.id != round.binding.id()?
            || path.file_name().unwrap().to_string_lossy() != format!("{}.json", round.id)
        {
            return Err("RUN_FOLLOWUP_BINDING stage=followup_progress".into());
        }
        let job = root.join(&round.id);
        if round.binding.worker != worker {
            return Err("RUN_FOLLOWUP_BINDING stage=followup_progress".into());
        }
        if matches!(round.stage, Stage::Builder | Stage::Checks)
            && job.join("job.json").is_file()
            && !job.join("finished.json").exists()
        {
            rounds.push(round);
        }
    }
    rounds.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rounds)
}

fn identity(value: &serde_json::Value) -> Result<String> {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or("RUN_FOLLOWUP_NATIVE_ID stage=followup_progress".into())
}

#[cfg(test)]
#[path = "../../tests/native_controller_support/idle_followup.rs"]
mod tests;
