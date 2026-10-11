use super::*;
use crate::context_budget::{Calibration, REQUEST_TOKENS};
use serde_json::Value;
use std::time::Duration;
mod state;
use state::{Start, Window};
/// https://docs.typesafe.ai/models checked live 2026-10-11: 80 RPS / 100K TPS.
/// The pinned 2026-09-30 mirror says 40 RPS; it is not the current limit.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Limits {
    pub requests_per_second: f64,
    pub tokens_per_second: f64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            requests_per_second: 80.0,
            tokens_per_second: 100_000.0,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub estimated: f64,
    pub source: String,
    pub recorded_exact: Option<u64>,
    #[serde(default)]
    pub input_utf8_bytes: u64,
}
impl Tokens {
    pub fn estimate(body: &str, calibration: Option<&Calibration>) -> Result<Self> {
        let value: Value =
            serde_json::from_str(body).map_err(|_| Error::new("JEV_BATCH_REQUEST_JSON"))?;
        let questions = serde_json::from_value(value["questions"].clone())
            .map_err(|_| Error::new("JEV_BATCH_REQUEST_JSON"))?;
        let planned = calibration.and_then(|c| c.plan(&value["state"], &questions));
        let (estimated, source) = planned
            .map_or((REQUEST_TOKENS, "documented_64K_reservation"), |p| {
                (p.estimated_request_tokens, "calibrated_estimate")
            });
        if !estimated.is_finite() || estimated < 0.0 {
            return Err(Error::new("JEV_BATCH_ESTIMATE"));
        }
        Ok(Self {
            estimated,
            source: source.into(),
            recorded_exact: None,
            input_utf8_bytes: (value["state"].to_string().len()
                + serde_json::to_vec(&questions)
                    .map_err(|_| Error::new("JEV_BATCH_REQUEST_JSON"))?
                    .len()) as u64,
        })
    }
    fn charge(&self) -> f64 {
        self.recorded_exact.map_or(self.estimated, |n| n as f64)
    }
}
pub(super) struct Pacer {
    root: PathBuf,
}
impl Pacer {
    pub fn new(root: &Path) -> Result<Self> {
        store::private(root)?;
        Ok(Self { root: root.into() })
    }
    pub fn start(&self, root: &Path, request: &WireRequest) -> Result<Start> {
        loop {
            let _lock = store::lock(&self.root)?;
            let mut window = self.load()?;
            let now = store::now();
            window.prune(now);
            let wait = window.wait(now, request.tokens.charge());
            if wait.is_zero() {
                let start = window.admit(now, &request.id, root, &request.tokens);
                store::write(&self.root.join("window.json"), &window)?;
                store::event(
                    root,
                    &serde_json::json!({"kind":"start","request":request.id,
                    "ticket":start.ticket,"at_ms":now,"estimated_input_tokens":request.tokens.estimated,
                    "recorded_exact":request.tokens.recorded_exact,"estimate_source":request.tokens.source,
                    "peak_in_flight":window.active.values().filter(|f|f.review==root).count(),"limits":window.effective}),
                )?;
                return Ok(start);
            }
            drop(_lock);
            // Short polling observes exact-count corrections made by other processes.
            std::thread::sleep(wait.min(Duration::from_millis(25)));
        }
    }
    pub fn finish(&self, root: &Path, start: &Start, response: Option<&RawResponse>) -> Result<()> {
        let _lock = store::lock(&self.root)?;
        let mut window = self.load()?;
        let exact = response
            .and_then(|r| serde_json::from_str::<Value>(&r.body).ok())
            .and_then(|v| v["usage"]["input_tokens"].as_u64());
        if let Some(exact) = exact
            && let Some(saved) = window.starts.iter_mut().find(|s| s.ticket == start.ticket)
        {
            saved.charged = exact as f64;
        }
        if let Some(exact) = exact.filter(|n| *n > 0)
            && start.input_utf8_bytes > 0
        {
            store::write(
                &self.root.join("calibration.json"),
                &Calibration {
                    input_utf8_bytes: start.input_utf8_bytes,
                    input_tokens: exact,
                },
            )?;
        }
        window.active.remove(&start.ticket);
        if let Some(response) = response {
            window.feedback(response);
        }
        store::write(&self.root.join("window.json"), &window)?;
        store::event(
            root,
            &serde_json::json!({"kind":"finish","request":start.request,"ticket":start.ticket,
            "at_ms":store::now(),"status":response.map(|r|r.status),"headers":response.map(|r|&r.headers),
            "estimated_input_tokens":start.estimated,"exact_input_tokens":exact,
            "count_label":if exact.is_some(){"exact"}else{"estimated, unconfirmed"}}),
        )
    }
    fn load(&self) -> Result<Window> {
        Ok(store::read(&self.root.join("window.json"))?.unwrap_or_default())
    }
}
