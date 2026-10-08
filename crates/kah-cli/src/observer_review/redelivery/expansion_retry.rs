//! Backoff schedules host expansion work; it never cancels or caps a review.
use crate::Result;
use serde_json::{Value, json};
use std::path::Path;
pub fn due(root: &Path, request: &str) -> Result<bool> {
    let source: Option<Value> =
        crate::observer_review::state::read(&root.join("validation-current.json"))?;
    let key = json!({"request":request,"request_digest":source.as_ref().map(|v|&v["request_digest"]),"report_digest":source.as_ref().map(|v|&v["report_digest"])});
    let previous: Option<Value> =
        crate::observer_review::state::read(&root.join("expansion-retry.json"))?;
    let now = kah_observer::session::now_ms();
    let same = previous.as_ref().filter(|v| v["state"] == key);
    if same.is_some_and(|v| v["next_ms"].as_u64().is_some_and(|at| now < at)) {
        return Ok(false);
    }
    let attempts = same.and_then(|v| v["attempts"].as_u64()).unwrap_or(0) + 1;
    // Reuse the existing 15s delivery observation interval, increasing delay
    // with unchanged attempts. A source/report change resets it immediately.
    crate::unattended_report::save(
        root,
        "expansion-retry.json",
        &json!({"state":key,"attempts":attempts,"at_ms":now,"next_ms":now.saturating_add(15_000u64.saturating_mul(attempts))}),
    )?;
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_expansion_backs_off_and_changed_report_resets_immediately() {
        let worker = format!("expansion-backoff-{}", crate::relay_store::nonce());
        let store = crate::observer_review::state::Store::open(&worker).unwrap();
        crate::unattended_report::save(
            &store.directory,
            "validation-current.json",
            &json!({"request_digest":"source-a","report_digest":"report-a"}),
        )
        .unwrap();
        assert!(due(&store.directory, "review-a").unwrap());
        assert!(!due(&store.directory, "review-a").unwrap());
        crate::unattended_report::save(
            &store.directory,
            "validation-current.json",
            &json!({"request_digest":"source-a","report_digest":"report-b"}),
        )
        .unwrap();
        assert!(due(&store.directory, "review-a").unwrap());
        let root = store.directory.parent().unwrap().to_owned();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
