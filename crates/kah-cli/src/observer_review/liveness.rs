//! No-progress observation, never a deadline on an active native/model review.
use serde::{Deserialize, Serialize};
mod monitor;
pub use monitor::{notice, processing, run};

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
pub fn transition(previous: Option<&Progress>, sample: &Sample) -> Progress {
    let same = previous.filter(|p| p.sample.request == sample.request);
    let grew = same.is_some_and(|p| {
        p.sample.transcript_version != sample.transcript_version
            || p.sample.report_version != sample.report_version
    });
    let active = sample.native_active || sample.controller_active || grew;
    let last_progress_ms = if active {
        sample.now_ms
    } else {
        same.map(|p| p.last_progress_ms).unwrap_or_else(|| {
            sample
                .started_ms
                .max(sample.transcript_version.map(|(_, t)| t).unwrap_or(0))
                .max(sample.report_version.map(|(_, t)| t).unwrap_or(0))
        })
    };
    // Reuse the native delivery readiness observation interval. This causes a
    // notice/retry ONLY with no activity; it never cancels or fails a review.
    let stalled = !active && sample.now_ms.saturating_sub(last_progress_ms) >= 15_000;
    let changed = stalled != same.is_some_and(|p| p.stalled);
    Progress {
        sample: sample.clone(),
        last_progress_ms,
        stalled,
        changed,
    }
}
