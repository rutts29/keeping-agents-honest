//! Atomic, typed judgments. Generated intent text remains a separate operation.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};
pub mod jev;
mod types;
mod validation;
pub use types::*;
pub use validation::{validate_request, validate_response};

pub trait Judge {
    fn evaluate(&self, state: &Value, questions: &Questions) -> Result<Judgment, Error>;
}

pub type Questions = BTreeMap<String, Question>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Judgment {
    pub backend: String,
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub metrics: Metrics,
    pub fallback_reason: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    #[serde(default = "one_call")]
    pub calls: u64,
    #[serde(default)]
    pub failed_calls: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub elapsed_ms: u64,
    pub estimated_cost_usd: Option<f64>,
}
fn one_call() -> u64 {
    1
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: String,
    pub stage: &'static str,
    pub status: Option<u16>,
    pub numeric_detail: Option<String>,
}
impl Error {
    pub fn new(code: &'static str) -> Self {
        Self {
            code: code.into(),
            stage: "judge",
            status: None,
            numeric_detail: None,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} stage={}", self.code, self.stage)?;
        if let Some(status) = self.status {
            write!(f, " http_status={status}")?;
        }
        if let Some(detail) = &self.numeric_detail {
            write!(f, " {detail}")?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

pub mod decision;
pub mod questions;
pub mod routing;
