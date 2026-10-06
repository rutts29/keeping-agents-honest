//! No-progress observation, never a deadline on an active native/model review.
use serde::{Deserialize, Serialize};
mod failure;
mod monitor;
pub(crate) mod rejection;
pub use monitor::{notice, processing, run, start, watch};

#[derive(Clone, Serialize, Deserialize)]
pub struct Sample {
    pub request: String,
    pub now_ms: u64,
    pub started_ms: u64,
    pub native_active: bool,
    pub controller_active: bool,
    pub transcript_version: Option<(u64, u64)>,
    pub report_version: Option<(u64, u64)>,
    pub cause: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Progress {
    pub sample: Sample,
    pub last_progress_ms: u64,
    pub stalled: bool,
    pub changed: bool,
}
fn validation_blocked(sample: &Sample) -> bool {
    sample.cause.starts_with("VALIDATION BLOCKED: ")
}
pub fn transition(previous: Option<&Progress>, sample: &Sample) -> Progress {
    let same = previous.filter(|p| p.sample.request == sample.request);
    let grew = same.is_some_and(|p| {
        p.sample.transcript_version != sample.transcript_version
            || p.sample.report_version != sample.report_version
    });
    let last_progress_ms = if sample.controller_active {
        sample.now_ms
    } else if grew {
        // A watcher resuming after a gap must use when those records were
        // written, not pretend that observing old growth is fresh activity.
        let written = sample
            .transcript_version
            .map(|(_, time)| time)
            .unwrap_or(0)
            .max(sample.report_version.map(|(_, time)| time).unwrap_or(0));
        written
            .max(same.unwrap().last_progress_ms)
            .min(sample.now_ms)
    } else {
        same.map(|p| p.last_progress_ms).unwrap_or(sample.now_ms)
    };
    // Reuse the native delivery readiness observation interval. This causes a
    // notice/retry ONLY with no activity; it never cancels or fails a review.
    // Working alone may mean a hung native request. Five minutes of record
    // silence only notifies a human; this is never a review deadline.
    let observation_ms = if sample.native_active {
        300_000
    } else {
        15_000
    };
    let stalled = same.is_some()
        && !validation_blocked(sample)
        && !sample.controller_active
        && sample.now_ms.saturating_sub(last_progress_ms) >= observation_ms;
    let changed = stalled != same.is_some_and(|p| p.stalled)
        || validation_blocked(sample) != same.is_some_and(|p| validation_blocked(&p.sample))
        || validation_blocked(sample) && same.is_some_and(|p| p.sample.cause != sample.cause)
        || stalled && same.is_some_and(|p| p.sample.native_active != sample.native_active);
    Progress {
        sample: sample.clone(),
        last_progress_ms,
        stalled,
        changed,
    }
}
