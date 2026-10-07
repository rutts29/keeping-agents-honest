//! HTTP account state, not a judgment or a report-validation error.
//! Pinned TypeSafe pages/api.md: HTTP errors use standard status codes.
use super::Error;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn path(home: &Path) -> std::path::PathBuf {
    home.join(".local/state/kah/jev-service.json")
}
pub fn unavailable(home: &Path) -> Result<bool, Error> {
    Ok(snapshot(home)?.is_some_and(|v| v["http_status"] == 402))
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
    let target = path(home);
    if snapshot(home)?.is_some_and(|v| v["http_status"] == json!(status)) {
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
    file.write_all(
        &serde_json::to_vec(&json!({"http_status":status,
        "observed_ms":crate::session::now_ms(),"source":"Jev HTTP response"}))
        .unwrap(),
    )
    .and_then(|_| file.sync_all())
    .map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    fs::rename(temporary, target).map_err(|_| Error::new("JEV_SERVICE_STATE"))?;
    Ok(())
}
