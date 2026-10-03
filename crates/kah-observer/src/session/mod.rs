//! Per-session observer state; model inference never supplies user source records.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod inherited;
mod sources;
mod storage;
pub use storage::Store;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub session: String,
    pub project: PathBuf,
    pub codex_home: PathBuf,
    pub native: PathBuf,
    pub model: String,
    pub effort: String,
    pub timeout_secs: u64,
    #[serde(default)]
    pub judge: crate::judge::routing::Backend,
    #[serde(default = "default_jev_model")]
    pub jev_model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Flight {
    pub pid: u32,
    pub process_stamp: String,
    pub turn: String,
    pub started_ms: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Gap {
    pub code: String,
    pub detail: String,
    pub at_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Usage {
    pub calls: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub elapsed_ms: u64,
    pub incomplete_calls: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub binding: Binding,
    pub run_id: Option<String>,
    #[serde(default)]
    pub compaction: Option<kah_core::projection::CompactedIntent>,
    pub action_digest: Option<String>,
    pub inflight: Option<Flight>,
    pub gap: Option<Gap>,
    pub drift_active: bool,
    pub reminded_turn: Option<String>,
    pub reminders: u64,
    pub observations: u64,
    pub usage: Usage,
    #[serde(default)]
    pub judgments: Vec<crate::judge::Judgment>,
    #[serde(default)]
    pub failures: Vec<serde_json::Value>,
}
impl State {
    pub fn new(binding: Binding) -> Self {
        Self {
            binding,
            run_id: None,
            compaction: None,
            action_digest: None,
            inflight: None,
            gap: None,
            drift_active: false,
            reminded_turn: None,
            reminders: 0,
            observations: 0,
            usage: Usage::default(),
            judgments: Vec::new(),
            failures: Vec::new(),
        }
    }
    pub fn begin(&mut self, turn: &str) {
        let pid = std::process::id();
        self.inflight = Some(Flight {
            pid,
            process_stamp: process_stamp(pid).unwrap_or_default(),
            turn: turn.into(),
            started_ms: now_ms(),
        });
    }
    pub fn fail(&mut self, code: &str, detail: impl Into<String>) {
        self.inflight = None;
        self.gap = Some(Gap {
            code: code.into(),
            detail: detail.into(),
            at_ms: now_ms(),
        });
    }
    pub fn recover_interrupted(&mut self) -> bool {
        let Some(flight) = &self.inflight else {
            return false;
        };
        if process_stamp(flight.pid).as_deref() == Some(&flight.process_stamp) {
            return false;
        }
        self.fail(
            "OBSERVER_INTERRUPTED",
            "The last observation process ended before committing its result.",
        );
        true
    }
}

pub fn process_stamp(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.get(stat.rfind(')')? + 2..)?
        .split_whitespace()
        .nth(19)
        .map(str::to_owned)
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn default_jev_model() -> String {
    "jev-latest".into()
}

/// Explicit, session-local operator pause; never inferred from elapsed time.
pub fn paused(base: &std::path::Path, session: &str) -> bool {
    Store::directory(base, session)
        .join("observer-paused")
        .is_file()
}
