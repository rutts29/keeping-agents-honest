//! Current rules are derived only from canonical active user intent.
use crate::Result;
use kah_core::{
    intent_history::{History, IntentKind},
    run_store::RunStore,
};
use kah_observer::{rule_source::atomization::Item, session};

pub fn history(worker: &str) -> Result<History> {
    let root = RunStore::default_state_root()?;
    let state =
        session::Store::read(&root, worker)?.ok_or("COMMIT_WORKER_MISSING stage=authority")?;
    let id = state
        .run_id
        .as_ref()
        .ok_or("COMMIT_NO_INTENT stage=authority")?;
    let store = RunStore::open_wait(&root, &state.binding.project, id)?;
    Ok(store.recover()?.history)
}
pub fn items(history: &History) -> Vec<Item> {
    history
        .snapshot()
        .intents
        .values()
        .filter(|i| {
            i.is_active() && matches!(i.kind, IntentKind::Constraint | IntentKind::Invariant)
        })
        .map(|i| Item {
            id: i.id.to_string(),
            kind: i.kind.clone(),
            text: i.text.clone(),
        })
        .collect()
}
