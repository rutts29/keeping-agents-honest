//! Fixed authenticated origin; the explicit replay constructor has no credentials.
use super::*;
use crate::judge::batch::{RawResponse, Transport, WireRequest};
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf, sync::Mutex};
pub struct HttpClient {
    agent: ureq::Agent,
    authorization: Option<ureq::http::HeaderValue>,
    endpoint: String,
    error_response: Mutex<Option<Value>>,
    pub(super) rate_root: Option<PathBuf>,
}
impl HttpClient {
    pub fn last_error_response(&self) -> Option<Value> {
        self.error_response
            .lock()
            .ok()
            .and_then(|saved| saved.clone())
    }
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
        let mut client = Self::configured(
            "https://api.typesafe.ai/v1/systemone".into(),
            Some(authorization),
        );
        client.rate_root = Some(
            kah_core::run_store::RunStore::default_state_root()
                .map_err(|_| Error::new("JEV_BATCH_DIRECTORY"))?
                .join("jev-rate"),
        );
        Ok(client)
    }
    /// Loopback only, no key lookup and no Authorization header. No CLI override.
    pub fn replay(address: SocketAddr) -> Result<Self, Error> {
        if !address.ip().is_loopback() {
            return Err(Error::new("JEV_REPLAY_ORIGIN"));
        }
        Ok(Self::configured(
            format!("http://{address}/v1/systemone"),
            None,
        ))
    }
    fn configured(endpoint: String, authorization: Option<ureq::http::HeaderValue>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            authorization,
            endpoint,
            error_response: Mutex::new(None),
            rate_root: None,
        }
    }
    pub fn post_raw(&self, body: &str) -> Result<Value, Error> {
        *self
            .error_response
            .lock()
            .map_err(|_| Error::new("JEV_RESPONSE_STATE"))? = None;
        let response = self.paced_scalar(body)?;
        let parsed = serde_json::from_str::<Value>(&response.body);
        if !(200..300).contains(&response.status) {
            let parsed = parsed.unwrap_or(Value::String(response.body));
            *self
                .error_response
                .lock()
                .map_err(|_| Error::new("JEV_RESPONSE_STATE"))? = Some(
                serde_json::json!({"status":response.status,"body":parsed,"headers":response.headers}),
            );
            return Err(Error {
                code: if response.status == 413 || super::context_rejection(&parsed) {
                    "JEV_CONTEXT_LIMIT"
                } else {
                    "JEV_HTTP_STATUS"
                }
                .into(),
                stage: "judge_http",
                status: Some(response.status),
                numeric_detail: None,
            });
        }
        parsed.map_err(|_| Error::new("JEV_RESPONSE_READ"))
    }
    pub(super) fn exchange_body(&self, body: &str, id: &str) -> Result<RawResponse, Error> {
        let mut request = self
            .agent
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header("X-KAH-Request-ID", id);
        if let Some(authorization) = &self.authorization {
            request = request.header("Authorization", authorization.clone());
        }
        let mut response = request.send(body.as_bytes()).map_err(super::http_error)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter(|(name, _)| {
                let name = name.as_str();
                name.starts_with("x-ratelimit-")
                    || name.starts_with("ratelimit")
                    || matches!(name, "retry-after" | "retry-after-ms" | "x-request-id")
            })
            .map(|(name, value)| {
                (
                    name.to_string(),
                    value
                        .to_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|_| format!("{:?}", value.as_bytes())),
                )
            })
            .collect::<BTreeMap<_, _>>();
        // ureq 3.4.2 BodyWithConfig defaults to unlimited; no response text cap.
        let body = response
            .body_mut()
            .with_config()
            .read_to_string()
            .map_err(|_| Error::new("JEV_RESPONSE_READ"))?;
        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
}
impl Client for HttpClient {
    fn post(&self, body: &Value) -> Result<Value, Error> {
        self.post_raw(&body.to_string())
    }
}
impl Transport for HttpClient {
    fn exchange(&self, request: &WireRequest) -> Result<RawResponse, Error> {
        self.exchange_body(&request.body, &request.id)
    }
}
