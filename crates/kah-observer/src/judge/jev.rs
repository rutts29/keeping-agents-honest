//! Documented HTTP contract: https://docs.typesafe.ai/api . No SDK or shell key.
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};
mod http;
mod scalar;
pub use http::HttpClient;

pub trait Client {
    fn post(&self, body: &Value) -> Result<Value, Error>;
}
pub struct Jev<C: Client> {
    pub client: C,
    pub model: String,
}
impl<C: Client> Judge for Jev<C> {
    fn evaluate(&self, state: &Value, questions: &Questions) -> Result<Judgment, Error> {
        validate_request(state, questions)?;
        let started = Instant::now();
        let response = self
            .client
            .post(&serde_json::json!({"model":self.model,"state":state,"questions":questions}))?;
        parse_response(
            state,
            questions,
            &response,
            started.elapsed().as_millis() as u64,
        )
    }
}
/// Shared scalar/batch decoder. Timing is telemetry, never judgment content.
pub fn parse_response(
    state: &Value,
    questions: &Questions,
    response: &Value,
    elapsed_ms: u64,
) -> Result<Judgment, Error> {
    let answers = serde_json::from_value(response["answers"].clone())
        .map_err(|_| Error::new("JEV_RESPONSE_SCHEMA"))?;
    validate_response(questions, &answers)?;
    let model = response["model"]
        .as_str()
        .filter(|s| s.starts_with("jev-") && s.len() < 100)
        .ok_or(Error::new("JEV_RESPONSE_MODEL"))?
        .to_owned();
    let input = response["usage"]["input_tokens"]
        .as_u64()
        .ok_or(Error::new("JEV_RESPONSE_USAGE"))?;
    let output = response["usage"]["output_tokens"]
        .as_u64()
        .ok_or(Error::new("JEV_RESPONSE_USAGE"))?;
    // Published 2026-09-29 price for 1.13.0: $0.042/M input, output free.
    // An alias may change models; do not silently apply an old price then.
    let cost = (model == "jev-1.13.0").then_some(input as f64 * 0.042 / 1_000_000.0);
    Ok(Judgment {
        backend: "jev".into(),
        model,
        answers,
        metrics: Metrics {
            input_utf8_bytes: Some(
                (state.to_string().len()
                    + serde_json::to_vec(questions)
                        .expect("question serialization")
                        .len()) as u64,
            ),
            calls: 1,
            input_tokens: input,
            output_tokens: output,
            elapsed_ms,
            estimated_cost_usd: cost,
            ..Metrics::default()
        },
        fallback_reason: None,
    })
}

pub(crate) fn context_rejection(body: &Value) -> bool {
    match body {
        Value::String(text) => {
            let text = text.to_ascii_lowercase();
            (text.contains("token") || text.contains("context"))
                && (text.contains("limit")
                    || text.contains("exceed")
                    || text.contains("too long")
                    || text.contains("maximum"))
        }
        Value::Array(values) => values.iter().any(context_rejection),
        Value::Object(fields) => fields
            .iter()
            .filter(|(key, _)| {
                matches!(
                    key.as_str(),
                    "error" | "detail" | "message" | "msg" | "code" | "type" | "error_type"
                )
            })
            .any(|(_, value)| context_rejection(value)),
        _ => false,
    }
}

#[cfg(test)]
#[path = "tests/context_rejection.rs"]
mod context_tests;
fn http_error(error: ureq::Error) -> Error {
    // Never render library errors or vendor error bodies: either can contain
    // request material. Status and stable reason are enough to route fallback.
    match error {
        ureq::Error::StatusCode(status) => Error {
            code: "JEV_HTTP_STATUS".into(),
            stage: "judge_http",
            status: Some(status),
            numeric_detail: None,
        },
        ureq::Error::Timeout(_) => Error::new("JEV_HTTP_TIMEOUT"),
        _ => Error::new("JEV_HTTP_TRANSPORT"),
    }
}
