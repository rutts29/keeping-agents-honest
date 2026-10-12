//! Opt-in fixture money guard; reservations change WHEN, never WHAT is sent.
use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
// https://docs.typesafe.ai/models checked2026-10-12: $0.042/M input, output free.
const NANODOLLARS_PER_TOKEN: u64 = 42;
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Budget {
    root: PathBuf,
    stop: u64,
}
pub(super) struct Lease {
    root: PathBuf,
    ticket: String,
    stop: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Ledger {
    input_tokens: u64,
    reserved: BTreeMap<String, f64>,
    usage_gap: bool,
}
impl Budget {
    pub fn from_env(rate_root: &Path) -> Result<Option<Self>> {
        let Some(value) = std::env::var_os("JEV_FIXTURE_MONEY_STOP") else {
            return Ok(None);
        };
        let usd = value
            .to_str()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n > 0.0 && *n * 1e9 < u64::MAX as f64)
            .ok_or(Error::new("JEV_FIXTURE_MONEY_CONFIG"))?;
        let stop = (usd * 1e9).round() as u64;
        if stop == 0 {
            return Err(Error::new("JEV_FIXTURE_MONEY_CONFIG"));
        }
        let root = std::env::var_os("KAH_FIXTURE_BUDGET_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| rate_root.join("fixture-budget"));
        Ok(Some(Self { root, stop }))
    }
    pub fn reserve(&self, request: &WireRequest) -> Result<Lease> {
        let _lock = store::lock(&self.root)?;
        let mut ledger = load(&self.root)?;
        let estimate = request
            .tokens
            .recorded_exact
            .map_or(request.tokens.estimated, |n| n as f64);
        if !estimate.is_finite() || estimate < 0.0 {
            return Err(Error::new("JEV_BATCH_ESTIMATE"));
        }
        let used = ledger.input_tokens as f64 + ledger.reserved.values().sum::<f64>();
        let price = NANODOLLARS_PER_TOKEN as f64;
        if ledger.usage_gap
            || used * price >= self.stop as f64
            || (used + estimate) * price > self.stop as f64
        {
            store::event(
                &self.root,
                &json!({"kind":"budget_refused","request":request.id,
                "at_ms":store::now(),"actual_input_tokens":ledger.input_tokens,
                "reserved_estimated_input_tokens":ledger.reserved.values().sum::<f64>(),
                "new_estimated_input_tokens":estimate,"stop_nanodollars":self.stop,"usage_gap":ledger.usage_gap}),
            )?;
            return Err(Error::new("JEV_FIXTURE_MONEY_STOP"));
        }
        let ticket = format!(
            "{}-{}-{}",
            std::process::id(),
            store::now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        ledger.reserved.insert(ticket.clone(), estimate);
        save(&self.root, &ledger, self.stop)?;
        store::event(
            &self.root,
            &json!({"kind":"budget_reserved","ticket":ticket,"request":request.id,
            "at_ms":store::now(),"estimated_input_tokens":request.tokens.estimated,
            "recorded_exact":request.tokens.recorded_exact,"estimate_source":request.tokens.source,
            "stop_nanodollars":self.stop}),
        )?;
        Ok(Lease {
            root: self.root.clone(),
            ticket,
            stop: self.stop,
        })
    }
}
impl Lease {
    pub fn record(self, request: &WireRequest, response: Option<&RawResponse>) -> Result<()> {
        let _lock = store::lock(&self.root)?;
        let mut ledger = load(&self.root)?;
        let reserved = ledger
            .reserved
            .remove(&self.ticket)
            .ok_or(Error::new("JEV_FIXTURE_MONEY_STATE"))?;
        let exact = response
            .and_then(|r| serde_json::from_str::<Value>(&r.body).ok())
            .and_then(|v| v["usage"]["input_tokens"].as_u64());
        if let Some(tokens) = exact {
            ledger.input_tokens = ledger
                .input_tokens
                .checked_add(tokens)
                .ok_or(Error::new("JEV_FIXTURE_MONEY_STATE"))?;
        } else if response.is_some_and(|r| (200..300).contains(&r.status)) {
            ledger.usage_gap = true;
            ledger.reserved.insert(self.ticket.clone(), reserved);
        }
        store::event(
            &self.root,
            &json!({"kind":"budget_finish","request":request.id,"ticket":self.ticket,
            "at_ms":store::now(),"status":response.map(|r|r.status),"exact_input_tokens":exact,
            "estimated_input_tokens":reserved,"count_label":if exact.is_some(){"exact"}else{"estimated, unconfirmed"},
            "actual_input_tokens":ledger.input_tokens,"usage_gap":ledger.usage_gap}),
        )?;
        save(&self.root, &ledger, self.stop)
    }
}
fn load(root: &Path) -> Result<Ledger> {
    let ledger: Ledger = store::read(&root.join("total.json"))?.unwrap_or_default();
    if ledger.reserved.values().any(|n| !n.is_finite() || *n < 0.0) {
        return Err(Error::new("JEV_FIXTURE_MONEY_STATE"));
    }
    Ok(ledger)
}
fn save(root: &Path, ledger: &Ledger, stop: u64) -> Result<()> {
    let mut value =
        serde_json::to_value(ledger).map_err(|_| Error::new("JEV_FIXTURE_MONEY_STATE"))?;
    value["usd"] = json!(ledger.input_tokens as f64 * NANODOLLARS_PER_TOKEN as f64 / 1e9);
    value["reserved_estimated_usd"] =
        json!(ledger.reserved.values().sum::<f64>() * NANODOLLARS_PER_TOKEN as f64 / 1e9);
    value["money_stop_usd"] = json!(stop as f64 / 1e9);
    store::write(&root.join("total.json"), &value)
}
