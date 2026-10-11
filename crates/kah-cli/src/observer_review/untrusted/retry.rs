//! Existing outage policy: pending/backoff, with no give-up deadline.
use crate::Result;
use serde_json::json;
use std::{path::Path, time::Duration};
pub fn run<T>(
    worker: &str,
    root: &Path,
    head: &str,
    mut action: impl FnMut() -> Result<T>,
) -> Result<T> {
    let pending: Option<serde_json::Value> = super::super::state::read(&root.join("pending.json"))?;
    let pending = pending.filter(|v| v["head"] == head);
    let mut attempts = pending
        .as_ref()
        .and_then(|v| v["attempts"].as_u64())
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let mut next = pending
        .as_ref()
        .and_then(|v| v["next_retry_ms"].as_u64())
        .unwrap_or(0);
    loop {
        if kah_observer::session::paused(
            &kah_core::run_store::RunStore::default_state_root()?,
            worker,
        ) {
            return Err("REVIEW_PAUSED stage=untrusted_commit".into());
        }
        let now = kah_observer::session::now_ms();
        if now < next {
            std::thread::sleep(Duration::from_millis(next - now));
            continue;
        }
        match action() {
            Ok(value) => return Ok(value),
            Err(error) if kah_observer::judge::availability::outage(&error.to_string()) => {
                let delay =
                    15_000u64.saturating_mul(2u64.checked_pow(attempts).unwrap_or(u64::MAX));
                attempts = attempts.saturating_add(1);
                next = now.saturating_add(delay);
                crate::unattended_report::save(
                    root,
                    "pending.json",
                    &json!({"head":head,"error":error.to_string(),"attempts":attempts,"retry_after_ms":delay,"next_retry_ms":next,"status":"pending","answered_receipts_retained":true}),
                )?;
                std::thread::sleep(Duration::from_millis(delay));
            }
            Err(error) => return Err(error),
        }
    }
}
