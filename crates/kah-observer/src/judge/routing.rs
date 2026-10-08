use super::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    // Old stored bindings migrate to the current user policy. Historical
    // judgments retain their original backend string and cannot gate actions.
    #[default]
    #[serde(alias = "luna")]
    Jev,
}
impl Backend {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "jev" => Ok(Self::Jev),
            _ => Err(Error::new("JUDGE_BACKEND")),
        }
    }
    pub fn for_domain(_domain: &str, explicit: Option<&str>) -> Result<Self, Error> {
        explicit.map_or(Ok(Self::Jev), Self::parse)
    }
}

pub fn evaluate(
    home: &Path,
    backend: Backend,
    jev_model: &str,
    state: &Value,
    questions: &Questions,
) -> Result<Judgment, Error> {
    availability::recovery::check(home)?;
    if availability::unavailable(home)? {
        return Err(Error::new("JEV_SERVICE_UNAVAILABLE"));
    }
    match backend {
        Backend::Jev => {
            let judge = jev::Jev {
                client: jev::HttpClient::from_home(home)?,
                model: jev_model.into(),
            };
            let result = retry(|| judge.evaluate(state, questions), std::thread::sleep);
            if result.is_ok() {
                availability::record(home, None)?;
            } else if let Err(error) = &result {
                availability::record_error(home, &error.to_string())?;
            }
            result
        }
    }
}

fn retry(
    mut call: impl FnMut() -> Result<Judgment, Error>,
    mut sleep: impl FnMut(Duration),
) -> Result<Judgment, Error> {
    let started = Instant::now();
    for attempt in 1..=3 {
        match call() {
            Ok(mut judgment) => {
                judgment.metrics.calls = attempt;
                judgment.metrics.failed_calls = attempt - 1;
                judgment.metrics.elapsed_ms = started.elapsed().as_millis() as u64;
                return Ok(judgment);
            }
            Err(error) if attempt < 3 && transient(&error) => {
                sleep(Duration::from_millis(if attempt == 1 { 250 } else { 1000 }));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("final attempt returns its result")
}
fn transient(error: &Error) -> bool {
    matches!(
        error.code.as_str(),
        "JEV_HTTP_TIMEOUT" | "JEV_HTTP_TRANSPORT"
    ) || error
        .status
        .is_some_and(|s| matches!(s, 408 | 429) || (500..600).contains(&s))
}

#[cfg(test)]
#[path = "tests/retries.rs"]
mod tests;
