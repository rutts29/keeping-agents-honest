//! Shared service probe, not a native observer turn or a deadline on a review.
//! TypeSafe documents exponential backoff for 529 overload.
use super::*;
use crate::judge::{Judge, Question, Questions, jev};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};

pub fn check(home: &Path) -> Result<(), Error> {
    with_probe(home, crate::session::now_ms(), || {
        let judge = jev::Jev {
            client: jev::HttpClient::from_home(home)?,
            model: "jev-1.13.0".into(),
        };
        let questions = Questions::from([(
            "service_health".into(),
            Question::Noul {
                instructions: "Does `service_health` equal true?".into(),
            },
        )]);
        judge
            .evaluate(&json!({"service_health":true}), &questions)
            .map(|_| ())
    })
}
pub fn with_probe(
    home: &Path,
    now: u64,
    probe: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    let Some(epoch) = snapshot(home)?.filter(|v| v["blocked"] == true || v["http_status"] == 402)
    else {
        return Ok(());
    };
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path(home).with_extension("lock"))
        .map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(fs::TryLockError::WouldBlock) => return Ok(()),
        Err(_) => return Err(Error::new("JEV_SERVICE_STATE")),
    }
    let Some(mut state) = snapshot(home)?.filter(|v| {
        v["observed_ms"] == epoch["observed_ms"]
            && (v["blocked"] == true || v["http_status"] == 402)
    }) else {
        return Ok(());
    };
    if now
        < state["next_probe_ms"].as_u64().unwrap_or_else(|| {
            state["observed_ms"]
                .as_u64()
                .unwrap_or(now)
                .saturating_add(15_000)
        })
    {
        return Ok(());
    }
    let attempts = state["probe_attempts"]
        .as_u64()
        .unwrap_or(0)
        .saturating_add(1);
    let delay = 15_000u64.saturating_mul(
        2u64.checked_pow(attempts.saturating_sub(1).try_into().unwrap_or(u32::MAX))
            .unwrap_or(u64::MAX),
    );
    state["probe_attempts"] = attempts.into();
    state["next_probe_ms"] = now.saturating_add(delay).into();
    save(home, &state)?;
    match probe() {
        Ok(()) => record(home, None),
        Err(error) => {
            // Keep the retry schedule and outage epoch across changing 5xx.
            state["last_probe_error"] = error.to_string().into();
            save(home, &state)
        }
    }
}
