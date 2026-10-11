use super::*;
use serde_json::Value;
use std::{fs, time::Duration};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Telemetry {
    pub requests: usize,
    pub physical_starts: usize,
    pub cached_responses: usize,
    pub peak_in_flight: u64,
    pub wall_ms: u128,
    pub achieved_requests_per_second: f64,
    pub achieved_tokens_per_second: f64,
    pub tokens_per_second_label: String,
    pub rate_responses: usize,
    pub http_429: usize,
    pub http_529: usize,
    pub total_backoff_wait_ms: u64,
    pub input_tokens_exact: u64,
    pub input_tokens_estimated_unconfirmed: f64,
}
pub(super) fn summarize(root: &Path, requests: usize, elapsed: Duration) -> Result<Telemetry> {
    let data = fs::read_to_string(root.join("events.jsonl"))
        .map_err(|_| Error::new("JEV_BATCH_TELEMETRY"))?;
    let mut result = Telemetry {
        requests,
        wall_ms: elapsed.as_millis(),
        ..Telemetry::default()
    };
    let mut first = None;
    for line in data.lines() {
        let value: Value =
            serde_json::from_str(line).map_err(|_| Error::new("JEV_BATCH_TELEMETRY"))?;
        if let Some(at) = value["at_ms"].as_u64() {
            first = Some(first.map_or(at, |old: u64| old.min(at)));
        }
        match value["kind"].as_str() {
            Some("start") => {
                result.physical_starts += 1;
                result.peak_in_flight = result
                    .peak_in_flight
                    .max(value["peak_in_flight"].as_u64().unwrap_or(0));
            }
            Some("reused") => result.cached_responses += 1,
            Some("backoff") => {
                result.rate_responses += 1;
                result.http_429 += usize::from(value["status"] == 429);
                result.http_529 += usize::from(value["status"] == 529);
                result.total_backoff_wait_ms += value["wait_ms"].as_u64().unwrap_or(0);
            }
            Some("finish") => {
                if let Some(n) = value["exact_input_tokens"].as_u64() {
                    result.input_tokens_exact += n;
                } else {
                    result.input_tokens_estimated_unconfirmed +=
                        value["estimated_input_tokens"].as_f64().unwrap_or(0.0);
                }
            }
            _ => (),
        }
    }
    result.wall_ms = result
        .wall_ms
        .max(first.map_or(0, |at| store::now().saturating_sub(at) as u128));
    let seconds = result.wall_ms as f64 / 1000.0;
    result.tokens_per_second_label = if result.input_tokens_estimated_unconfirmed > 0.0 {
        "exact plus estimated, unconfirmed"
    } else {
        "exact"
    }
    .into();
    if seconds > 0.0 {
        result.achieved_requests_per_second = result.physical_starts as f64 / seconds;
        result.achieved_tokens_per_second = (result.input_tokens_exact as f64
            + result.input_tokens_estimated_unconfirmed)
            / seconds;
    }
    Ok(result)
}
