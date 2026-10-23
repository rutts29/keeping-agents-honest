//! HTTP account state, not a judgment or a report-validation error.
//! Documented TypeSafe HTTP contract (https://docs.typesafe.ai/api): HTTP errors
//! use standard status codes.
use super::Error;
use serde_json::{Value, json};
use std::{fs, path::Path};
pub mod recovery;

fn path(home: &Path) -> std::path::PathBuf {
    kah_core::run_store::RunStore::state_root_for(home).join("jev-service.json")
}
pub fn unavailable(home: &Path) -> Result<bool, Error> {
    Ok(snapshot(home)?.is_some_and(|v| v["http_status"] == 402 || v["blocked"] == true))
}
pub fn outage(error: &str) -> bool {
    error.starts_with("JEV_")
        && (error.starts_with("JEV_HTTP_TIMEOUT")
            || error.starts_with("JEV_HTTP_TRANSPORT")
            || error.starts_with("JEV_SERVICE_UNAVAILABLE")
            || error
                .split_whitespace()
                .find_map(|s| {
                    s.strip_prefix("http_status=")
                        .and_then(|s| s.parse::<u16>().ok())
                })
                .is_some_and(|s| matches!(s, 402 | 408) || (500..600).contains(&s)))
}
pub fn record_error(home: &Path, error: &str) -> Result<(), Error> {
    if !outage(error) {
        return Ok(());
    }
    let previous = snapshot(home)?;
    let status = error.split_whitespace().find_map(|s| {
        s.strip_prefix("http_status=")
            .and_then(|s| s.parse::<u16>().ok())
    });
    let observed = previous
        .as_ref()
        .filter(|v| v["blocked"] == true || v["http_status"] == 402)
        .and_then(|v| v["observed_ms"].as_u64())
        .unwrap_or_else(crate::session::now_ms);
    let mut state = previous
        .filter(|v| v["blocked"] == true)
        .unwrap_or_else(|| json!({}));
    state["blocked"] = true.into();
    state["http_status"] = json!(status);
    state["error"] = error.into();
    state["observed_ms"] = observed.into();
    state["source"] = "Jev HTTP response".into();
    save(home, &state)
}
pub fn snapshot(home: &Path) -> Result<Option<Value>, Error> {
    match fs::read(path(home)) {
        Ok(bytes) => {
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
            Ok(Some(value))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Error::new("JEV_SERVICE_STATE")),
    }
}
pub fn record(home: &Path, status: Option<u16>) -> Result<(), Error> {
    if let Some(status) = status {
        return record_error(
            home,
            &format!("JEV_HTTP_STATUS stage=judge_http http_status={status}"),
        );
    }
    if snapshot(home)?.is_some_and(|v| v["http_status"] == Value::Null && v["blocked"] != true) {
        return Ok(());
    }
    save(
        home,
        &json!({"http_status":null,"blocked":false,"observed_ms":crate::session::now_ms(),"source":"Jev HTTP response"}),
    )
}
pub(crate) fn save(home: &Path, value: &Value) -> Result<(), Error> {
    let target = path(home);
    if snapshot(home)?.as_ref() == Some(value) {
        return Ok(());
    }
    let root = target.parent().unwrap();
    kah_core::run_store::ensure_private_directory(root)
        .map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = target.with_extension(format!(
        "{}-{}-{}.tmp",
        std::process::id(),
        crate::session::now_ms(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    file.write_all(&serde_json::to_vec(value).unwrap())
        .and_then(|_| file.sync_all())
        .map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    fs::rename(temporary, target).map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    Ok(())
}
