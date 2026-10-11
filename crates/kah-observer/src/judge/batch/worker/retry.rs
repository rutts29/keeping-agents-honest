use super::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
pub(super) fn delay(headers: &BTreeMap<String, String>, attempt: u32) -> Duration {
    let millis = headers
        .get("retry-after-ms")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .and_then(|s| Duration::try_from_secs_f64(s / 1000.0).ok());
    let seconds = headers.get("retry-after").and_then(|s| {
        s.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .and_then(|n| Duration::try_from_secs_f64(n).ok())
            .or_else(|| {
                httpdate::parse_http_date(s)
                    .ok()
                    .map(|at| at.duration_since(SystemTime::now()).unwrap_or_default())
            })
    });
    if let Some(delay) = millis.into_iter().chain(seconds).max() {
        return delay;
    }
    // Official SDK defaults: 500ms, doubling to 5s, subtract up to 25% jitter.
    // https://docs.typesafe.ai/sdk/javascript/api/interfaces/RetryPolicy
    let base = (0.5 * 2.0_f64.powi(attempt.min(4) as i32)).min(5.0);
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    Duration::from_secs_f64(base * (1.0 - 0.25 * (tick % 1000) as f64 / 1000.0))
}
