//! Private state, with short writer locks. Inference never holds this lock.
use kah_core::{run_store::ensure_private_directory, unattended::RunControl};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

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
    pub fn create(base: &Path, id: &str, record: Record) -> Result<Self, String> {
        validate_id(id)?;
        record.validate()?;
        ensure_private_directory(base).map_err(|e| e.to_string())?;
        let path = base.join(id);
        fs::create_dir(&path).map_err(|e| format!("RUN_CREATE stage=run_store: {e}"))?;
        ensure_private_directory(&path).map_err(|e| e.to_string())?;
        let mut store = Self::open(base, id)?;
        store.save(&record)?;
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
        let result = (|| {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(io_error)?;
            serde_json::to_writer(&mut file, record)
                .map_err(|e| format!("RUN_STATE_JSON stage=run_store: {e}"))?;
            file.write_all(b"\n")
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

fn validate_id(id: &str) -> Result<(), String> {
    if !id.starts_with("run-")
        || id.len() <= 4
        || id.len() > 80
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
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
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("RUN_STATE_SIZE stage=run_store".into());
    }
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|e| format!("RUN_STATE_JSON stage=run_store: {e}"))?;
    record.validate()?;
    Ok(record)
}
