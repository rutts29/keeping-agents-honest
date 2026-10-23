//! Private state, with short writer locks. Inference never holds this lock.
use kah_core::{run_store::ensure_private_directory, unattended::RunControl};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// The largest `state.json` (including its newline) that `save` writes and
/// `read` accepts.
pub const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub project: PathBuf,
    pub domain: String,
    pub worktree: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub control: RunControl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<crate::native::Native>,
}
impl Record {
    pub fn published_status(&self) -> kah_core::unattended::RunStatus {
        use kah_core::unattended::RunStatus;
        let status = self.control.status();
        if self
            .native
            .as_ref()
            .is_some_and(|n| n.report_ready == Some(false))
            && !matches!(
                status,
                RunStatus::Created | RunStatus::Running | RunStatus::Stopping
            )
        {
            RunStatus::Stopping
        } else {
            status
        }
    }
    pub fn public_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        value["control"]["status"] = serde_json::to_value(self.published_status())?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), String> {
        if !self.project.is_absolute() || !self.worktree.is_absolute() {
            return Err("RUN_PATH stage=run_store: project and worktree must be absolute".into());
        }
        if !matches!(self.domain.as_str(), "mine" | "research" | "untrusted") {
            return Err("RUN_DOMAIN stage=run_store".into());
        }
        Ok(())
    }
}

pub struct Store {
    path: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open_wait(base: &Path, id: &str) -> Result<Self, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match Self::open(base, id) {
                Err(e) if e.starts_with("RUN_BUSY") && std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                result => return result,
            }
        }
    }
    pub fn create(base: &Path, id: &str, record: Record) -> Result<Self, String> {
        validate_id(id)?;
        record.validate()?;
        ensure_private_directory(base).map_err(|e| e.to_string())?;
        let path = base.join(id);
        fs::create_dir(&path).map_err(|e| format!("RUN_CREATE stage=run_store: {e}"))?;
        ensure_private_directory(&path).map_err(|e| e.to_string())?;
        let mut store = Self::open(base, id)?;
        if let Err(error) = store.save(&record) {
            // Only the directory this call just reserved is removed.
            drop(store);
            let _ = fs::remove_dir_all(&path);
            return Err(error);
        }
        Ok(store)
    }
    pub fn open(base: &Path, id: &str) -> Result<Self, String> {
        validate_id(id)?;
        let path = base.join(id);
        if !fs::symlink_metadata(&path).map_err(io_error)?.is_dir() {
            return Err("RUN_DIRECTORY stage=run_store: expected a real run directory".into());
        }
        ensure_private_directory(&path).map_err(|e| e.to_string())?;
        let lock = path.join("state.lock");
        reject_symlink(&lock)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(lock)
            .map_err(io_error)?;
        lock.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => {
                "RUN_BUSY stage=run_store: another state writer is active".into()
            }
            fs::TryLockError::Error(e) => io_error(e),
        })?;
        Ok(Self { path, _lock: lock })
    }
    pub fn load(&self) -> Result<Record, String> {
        read_record(&self.path)
    }
    pub fn read(base: &Path, id: &str) -> Result<Record, String> {
        read_record(&Self::path(base, id)?)
    }
    pub fn path(base: &Path, id: &str) -> Result<PathBuf, String> {
        validate_id(id)?;
        let path = base.join(id);
        if !fs::symlink_metadata(&path).map_err(io_error)?.is_dir() {
            return Err("RUN_DIRECTORY stage=run_store".into());
        }
        Ok(path)
    }
    pub fn save(&mut self, record: &Record) -> Result<(), String> {
        record.validate()?;
        let path = self.path.join("state.json");
        reject_symlink(&path)?;
        let temporary = self.path.join(format!("state-{}.tmp", std::process::id()));
        reject_symlink(&temporary)?;
        let mut bytes = serde_json::to_vec(record)
            .map_err(|e| format!("RUN_STATE_JSON stage=run_store: {e}"))?;
        bytes.push(b'\n');
        // Refuse before touching the last good state: read rejects larger files.
        if bytes.len() > MAX_STATE_BYTES {
            return Err(format!(
                "RUN_STATE_SIZE stage=run_store: state is {} bytes, limit {MAX_STATE_BYTES}",
                bytes.len()
            ));
        }
        let result = (|| {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(io_error)?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(io_error)?;
            fs::rename(&temporary, path).map_err(io_error)?;
            File::open(&self.path)
                .map_err(io_error)?
                .sync_all()
                .map_err(io_error)
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn directory(&self) -> &Path {
        &self.path
    }
    pub fn branch(&self) -> String {
        format!("kah/{}", self.path.file_name().unwrap().to_string_lossy())
    }
}

/// The one contract for an unattended run id: `run-` plus at least one
/// character, 80 bytes at most, ASCII letters, digits and `-` only.
pub fn valid_run_id(id: &str) -> bool {
    id.starts_with("run-")
        && id.len() > 4
        && id.len() <= 80
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn validate_id(id: &str) -> Result<(), String> {
    if !valid_run_id(id) {
        return Err("RUN_ID stage=run_store: invalid run identifier".into());
    }
    Ok(())
}
fn reject_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("RUN_STATE_SYMLINK stage=run_store".into());
    }
    Ok(())
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_STATE_IO stage=run_store: {error}")
}

fn read_record(directory: &Path) -> Result<Record, String> {
    let path = directory.join("state.json");
    reject_symlink(&path)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io_error)?
        .take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err("RUN_STATE_SIZE stage=run_store".into());
    }
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|e| format!("RUN_STATE_JSON stage=run_store: {e}"))?;
    record.validate()?;
    Ok(record)
}
