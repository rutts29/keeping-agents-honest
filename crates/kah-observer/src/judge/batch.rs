//! Independent immutable requests; rates govern starts, never payloads or in-flight count.
use super::Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
mod budget;
mod cache;
mod pacing;
mod store;
mod telemetry;
mod verdict;
mod worker;
pub use pacing::{Limits, Tokens};
pub use telemetry::Telemetry;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Serialize, Deserialize)]
pub struct WireRequest {
    pub id: String,
    pub body: String,
    pub tokens: Tokens,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RawResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}
pub trait Transport: Sync {
    fn exchange(&self, request: &WireRequest) -> Result<RawResponse>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub request: WireRequest,
    pub response: RawResponse,
    #[serde(default)]
    pub reused: bool,
    #[serde(default)]
    pub elapsed_ms: u64,
}
#[derive(Clone, Copy)]
pub enum Mode {
    Sequential,
    Concurrent,
}
pub struct Outcome {
    pub responses: Vec<Receipt>,
    pub telemetry: Telemetry,
}
impl Outcome {
    /// Identity excludes telemetry; original request and response bytes survive.
    pub fn evidence_bytes(&self) -> Result<Vec<u8>> {
        let payload = self
            .responses
            .iter()
            .map(|r| {
                serde_json::json!({
            "id":r.request.id,"request":r.request.body,"status":r.response.status,
            "raw_response":r.response.body,"verdict":r.verdict()})
            })
            .collect::<Vec<_>>();
        serde_json::to_vec(&payload).map_err(|_| Error::new("JEV_BATCH_SERIALIZE"))
    }
}
pub struct Batch {
    root: PathBuf,
    pacer: pacing::Pacer,
    budget: Option<budget::Budget>,
}
impl Batch {
    pub fn calibration(rate_root: &Path) -> Result<Option<crate::context_budget::Calibration>> {
        store::read(&rate_root.join("calibration.json"))
    }
    pub fn new(root: &Path, rate_root: &Path) -> Result<Self> {
        store::private(root)?;
        Ok(Self {
            root: root.into(),
            pacer: pacing::Pacer::new(rate_root)?,
            budget: budget::Budget::from_env(rate_root)?,
        })
    }
    pub fn run(
        &self,
        requests: &[WireRequest],
        transport: &impl Transport,
        mode: Mode,
    ) -> Result<Outcome> {
        let started = Instant::now();
        let responses = match mode {
            Mode::Sequential => requests
                .iter()
                .map(|r| worker::execute(self, r, transport))
                .collect::<Result<Vec<_>>>()?,
            Mode::Concurrent => std::thread::scope(|scope| {
                let handles = requests
                    .iter()
                    .map(|r| scope.spawn(move || worker::execute(self, r, transport)))
                    .collect::<Vec<_>>();
                // Join in input order even when network completion order differs.
                handles
                    .into_iter()
                    .map(|h| h.join().map_err(|_| Error::new("JEV_BATCH_WORKER"))?)
                    .collect::<Result<Vec<_>>>()
            })?,
        };
        let telemetry = telemetry::summarize(&self.root, requests.len(), started.elapsed())?;
        store::write(&self.root.join("telemetry.json"), &telemetry)?;
        Ok(Outcome {
            responses,
            telemetry,
        })
    }
}
