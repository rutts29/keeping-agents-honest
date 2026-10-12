use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(super) fn private(root: &Path) -> Result<()> {
    kah_core::run_store::ensure_private_directory(root)
        .map_err(|_| Error::new("JEV_BATCH_DIRECTORY"))
}
pub(super) fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(m) if m.is_file() && m.permissions().mode() & 0o077 == 0 => (),
        _ => return Err(Error::new("JEV_BATCH_PRIVATE")),
    }
    let bytes = fs::read(path).map_err(|_| Error::new("JEV_BATCH_READ"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| Error::new("JEV_BATCH_JSON"))
}
pub(super) fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let root = path.parent().ok_or(Error::new("JEV_BATCH_PATH"))?;
    kah_core::run_store::ensure_save_directory(root)
        .map_err(|_| Error::new("JEV_BATCH_DIRECTORY"))?;
    let temporary = root.join(format!(
        "{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let bytes = serde_json::to_vec(value).map_err(|_| Error::new("JEV_BATCH_SERIALIZE"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| Error::new("JEV_BATCH_WRITE"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| Error::new("JEV_BATCH_WRITE"))?;
    fs::rename(temporary, path).map_err(|_| Error::new("JEV_BATCH_WRITE"))?;
    File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::new("JEV_BATCH_WRITE"))
}
pub(super) fn lock(root: &Path) -> Result<File> {
    private(root)?;
    let path = root.join("lock");
    if fs::symlink_metadata(&path)
        .is_ok_and(|m| !m.is_file() || m.permissions().mode() & 0o077 != 0)
    {
        return Err(Error::new("JEV_BATCH_PRIVATE"));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|_| Error::new("JEV_BATCH_LOCK"))?;
    file.lock().map_err(|_| Error::new("JEV_BATCH_LOCK"))?;
    Ok(file)
}
pub(super) fn event(root: &Path, value: &serde_json::Value) -> Result<()> {
    let _lock = lock(&root.join("events-writer"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("events.jsonl"))
        .map_err(|_| Error::new("JEV_BATCH_EVENT"))?;
    writeln!(file, "{value}")
        .and_then(|()| file.sync_all())
        .map_err(|_| Error::new("JEV_BATCH_EVENT"))
}
pub(super) fn now() -> u64 {
    crate::session::now_ms()
}
