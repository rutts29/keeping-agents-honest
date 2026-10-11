pub use super::selection::accept;
use super::*;
use crate::observer_review::{
    preparation, source_store,
    state::{self, Store},
};
use serde_json::json;
pub fn pending(worker: &str, batch: &Batch) -> Result<bool> {
    for entry in &batch.decisions {
        if preparation::pending(worker, &entry.decision)? {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn prompt(worker: &str, batch: &Batch) -> Result<String> {
    let state = crate::observer_review::state::read_control(worker)?
        .ok_or("PREPARATION_BATCH_STATE stage=context_preparation")?;
    let directory = state.observer.workspace.join("inputs").join(&batch.review);
    let source = directory.join(format!("preparation-batch-{}.json", batch.id));
    source_store::save(
        &directory,
        source.file_name().unwrap().to_str().unwrap(),
        &serde_json::to_value(batch)?,
    )?;
    let mut paths = vec![];
    for entry in &batch.decisions {
        let job = load_job(worker, entry)?;
        let name = format!("preparation-{}.json", job.decision);
        source_store::save(&directory, &name, &serde_json::to_value(&job)?)?;
        paths.push(json!({"decision":job.decision,"source":directory.join(name)}));
    }
    let mut files = vec![source.clone()];
    files.extend(
        paths
            .iter()
            .filter_map(|row| row["source"].as_str().map(std::path::PathBuf::from)),
    );
    source_store::share_preparation(&Store::directory(worker)?, &directory, &files)?;
    kah_run::access::share_tree(&directory, kah_run::access::service_uid()?, false)?;
    let output = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("selection-{}.json", batch.id));
    let mut command = std::process::Command::new(state.observer.workspace.join("bin/kah"));
    command
        .args(["observe", "select-batch", "--request"])
        .arg(&source)
        .arg("--json")
        .arg(&output);
    Ok(format!(
        "kah review request {} (Preparation). ONE batched preparation wake for {} independent, unchanged Jev decisions. The batch index is {}. Original states, questions and chronology are separate; NEVER combine them into one decision, split a decision, summarize user words, or cut text. Jobs: {}. Read each source completely via bin/kah observe read-source PATH, including full referenced context. Select WHOLE relevant items for each job covering every question, retaining its original goal and ALL active user/relayed items. Each item must appear exactly once in selected or excluded, with id, digest, reason and superseded_by (null unless verified). Exclude only irrelevant non-user items with reasons. No scores or verdicts. Jev's documented token budgets are 32000 for state plus longest question and 64000 per complete request; no tokenizer exists, bytes are not a limit. Write an object {{\"batch\":\"{}\",\"selections\":[MANIFEST,...]}} with one manifest (decision, selected, excluded) per listed job. Submit ALL together using {} in the foreground. Require context_selection_submitted acknowledgement, finish that submission tool, then remain idle. Host judges each complete original decision separately; never poll or create a new session.",
        batch.id,
        batch.decisions.len(),
        source.display(),
        serde_json::to_string(&paths)?,
        batch.id,
        crate::native_run::shell_command(&command)?
    ))
}
pub fn reopen(worker: &str, decision: &str) -> Result<()> {
    let root = Store::directory(worker)?;
    let scoped = preparation::cursors::owner(worker, decision)?
        .map(|review| {
            preparation::active_decision(worker, &review).map(|id| json!({"decision":id}))
        })
        .transpose()?;
    let current: Option<serde_json::Value> = if scoped.is_some() {
        scoped
    } else {
        state::read(&root.join("preparation-current.json"))?
    };
    let Some(id) = current.as_ref().and_then(|v| v["decision"].as_str()) else {
        return Ok(());
    };
    let Some(batch) = load(worker, id)? else {
        return Ok(());
    };
    if !batch.decisions.iter().any(|e| e.decision == decision) {
        return Ok(());
    }
    let state = crate::observer_review::state::read_control(worker)?
        .ok_or("PREPARATION_BATCH_STATE stage=context_preparation")?;
    // Retain the old ACK/manifest as evidence; a rejected complete decision
    // needs a new batch selection, without rerunning successful Jev decisions.
    let archive = root
        .join("preparation-batch-retries")
        .join(crate::relay_store::nonce());
    kah_core::run_store::ensure_private_directory(&archive)?;
    for path in [
        root.join(format!("preparation-woken-{id}.json")),
        state
            .observer
            .workspace
            .join("outbox")
            .join(format!("selected-{id}.json")),
    ] {
        if path.exists() {
            source_store::access_repair::archive(&path, &archive.join(path.file_name().unwrap()))?;
        }
    }
    let mut entries = vec![];
    for entry in &batch.decisions {
        if entry.decision == decision
            || preparation::pending(worker, &entry.decision)?
                && !root
                    .join(format!("selection-{}.json", entry.decision))
                    .is_file()
        {
            entries.push(entry);
        }
    }
    if !entries.is_empty() {
        stage_iter(
            worker,
            &batch.review,
            entries.into_iter().map(|entry| {
                state::read::<Job>(&root.join(format!("preparation-{}.json", entry.decision)))?
                    .ok_or_else(|| "PREPARATION_BATCH_JOB stage=context_preparation".into())
            }),
        )?;
    }
    Ok(())
}
