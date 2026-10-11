//! Decisive Step1: code resolves authority; native thinker atomizes, no judge.
use super::*;
use kah_core::intent_history::IntentKind;
pub use kah_observer::rule_source::atomization::{AtomicRule, Item, verify};
pub fn input(repo: &Path, run: &str, output: &Path) -> Result<Value> {
    let store = kah_core::run_store::RunStore::open_default(repo, run)?;
    let recovered = store.recover()?;
    let snapshot = recovered.history.snapshot();
    let items: Vec<_> = snapshot
        .intents
        .values()
        .filter(|item| {
            item.is_active() && matches!(item.kind, IntentKind::Constraint | IntentKind::Invariant)
        })
        .map(|item| Item {
            id: item.id.to_string(),
            kind: item.kind.clone(),
            text: item.text.clone(),
        })
        .collect();
    kah_core::run_store::ensure_private_directory(output)?;
    let body = serde_json::to_string(&items)?;
    save_text(output, "intent-only.json", &body)?;
    let meta = json!({"run":run,"revision":recovered.projection.revision,"source_store":store.path(),
        "history_digest":commit_check::digest(&serde_json::to_vec(recovered.history.records())?),
        "input_digest":commit_check::digest(body.as_bytes()),"items":items.len(),
        "included":"active canonical Constraint/Invariant only; no projection text compaction",
        "excluded":"goals, tasks, superseded/withdrawn items, evidence/verdicts, labels, plans, result files",
        "pending_events":recovered.pending_delta.len(),"actual_jev_calls":0});
    save_text(
        output,
        "resolution.json",
        &serde_json::to_string_pretty(&meta)?,
    )?;
    Ok(meta)
}
