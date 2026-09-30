//! One long-lived worker lease, separate from short operator state writes.
use crate::store::Store;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

pub struct Lease {
    _file: File,
}
impl Lease {
    pub fn acquire(base: &Path, id: &str) -> Result<Self, String> {
        let path = Store::path(base, id)?.join("worker.lock");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("RUN_LEASE_SYMLINK stage=worker_lease".into());
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("RUN_LEASE_IO stage=worker_lease: {e}"))?;
        file.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => {
                "RUN_WORKER_ACTIVE stage=worker_lease: another worker owns this run".into()
            }
            fs::TryLockError::Error(e) => format!("RUN_LEASE_IO stage=worker_lease: {e}"),
        })?;
        Ok(Self { _file: file })
    }
}
