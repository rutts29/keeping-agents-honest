//! A due checkpoint owns the wake; both original events and answers stay recorded.
use super::super::{
    queue::{self, Wake},
    source_store,
    state::{Purpose, State, Store},
};
use crate::Result;
use kah_observer::commit_review::Review;
use serde_json::{Value, json};
pub fn into_due(home: &std::path::Path, state: &State, review: &Review) -> Result<Option<Value>> {
    let worker = &state.worker.session;
    let root = Store::directory(worker)?;
    let entries = queue::entries(worker)?;
    for (path, mut wake) in entries {
        if wake.purpose != Purpose::Checkpoint || wake.trigger["review_class"] == "commit" {
            continue;
        }
        let head = super::super::queue::supersede::trigger_head(&wake.trigger);
        if let Some(head) = head
            && !super::capture::ancestor(home, state, &review.head, head)?
        {
            continue;
        }
        let folded = kah_observer::commit_review::checkpoint::folded(
            review,
            path.file_stem().and_then(|s| s.to_str()),
        )
        .unwrap();
        let original = serde_json::to_value(&wake)?;
        let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
        if !path.exists() {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let archive = root.join("option1-folds");
        kah_core::run_store::ensure_private_directory(&archive)?;
        crate::unattended_report::save(
            &archive,
            &format!("{}-{name}", review.head),
            &json!({"original":original,"folded":folded,"review":review}),
        )?;
        let mut records = wake.trigger["option1_folded"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        records.push(json!({"head":review.head,"record":review,"covered_by":name}));
        wake.trigger["option1_folded"] = json!(records);
        source_store::save(
            path.parent().unwrap(),
            name,
            &serde_json::to_value(Wake { ..wake })?,
        )?;
        return Ok(Some(folded));
    }
    Ok(None)
}
