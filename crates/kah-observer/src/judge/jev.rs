//! Documented HTTP contract: https://docs.typesafe.ai/api . No SDK or shell key.
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

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
                elapsed_ms: started.elapsed().as_millis() as u64,
                estimated_cost_usd: cost,
                ..Metrics::default()
            },
            fallback_reason: None,
        })
    }
}

pub struct HttpClient {
    agent: ureq::Agent,
    authorization: ureq::http::HeaderValue,
}
impl HttpClient {
    pub fn from_home(home: &Path) -> Result<Self, Error> {
        let path = home.join(".config/kah/typesafe.key");
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| Error::new("JEV_KEY_UNAVAILABLE"))?;
        if !metadata.is_file()
            || metadata.permissions().mode() & 0o777 != 0o600
            || metadata.len() > 8192
            || metadata.uid()
                != home
                    .metadata()
                    .map_err(|_| Error::new("JEV_KEY_OWNER"))?
                    .uid()
        {
            return Err(Error::new("JEV_KEY_PERMISSIONS"));
        }
        let key = fs::read_to_string(path).map_err(|_| Error::new("JEV_KEY_READ"))?;
        let key = key.trim();
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::new("JEV_KEY_FORMAT"));
        }
        let mut authorization = ureq::http::HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| Error::new("JEV_KEY_FORMAT"))?;
        authorization.set_sensitive(true);
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build();
        Ok(Self {
            agent: ureq::Agent::new_with_config(config),
            authorization,
        })
    }
}
impl Client for HttpClient {
    fn post(&self, body: &Value) -> Result<Value, Error> {
        // Fixed TLS origin and no redirects: neither project data nor config can
        // route this host-only credential to another endpoint.
        let mut response = self
            .agent
            .post("https://api.typesafe.ai/v1/systemone")
            .header("Authorization", self.authorization.clone())
            .send_json(body)
            .map_err(http_error)?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response
                .body_mut()
                .with_config()
                .limit(1024 * 1024)
                .read_json::<Value>()
                .ok();
            return Err(Error {
                code: if body.as_ref().is_some_and(context_rejection) {
                    "JEV_CONTEXT_LIMIT"
                } else {
                    "JEV_HTTP_STATUS"
                }
                .into(),
                stage: "judge_http",
                status: Some(status),
                numeric_detail: None,
            });
        }
        response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_json()
            .map_err(|_| Error::new("JEV_RESPONSE_READ"))
    }
}
fn context_rejection(body: &Value) -> bool {
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
                    "error" | "detail" | "message" | "msg" | "code" | "type"
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
