//! hook::spawn starts nothing while a job holds its lock (try_lock WouldBlock
//! returns Ok), so a job that exits while its input holds work loses that
//! wakeup until some later hook spawns it again.
use super::state::Store;
use crate::Result;
use std::{
    fs::{File, TryLockError},
    path::Path,
};

/// The lock this job inherited from hook::spawn (a fresh handle when run by hand).
pub fn lease(worker: &str, command: &str) -> Result<File> {
    crate::helper_upgrade::open_lease(
        &Store::directory(worker)?.join(format!("job-{command}.lock")),
    )
}

/// Run passes until no input is pending. After each pass the job releases its
/// lock and checks again; it continues only if it takes the lock back, so a
/// job spawned in between owns that input and nothing is processed twice.
pub fn until_settled(
    lease: &File,
    mut pass: impl FnMut() -> Result<()>,
    mut pending: impl FnMut() -> Result<bool>,
) -> Result<()> {
    loop {
        pass()?;
        lease.unlock()?;
        if !pending()? {
            return Ok(());
        }
        match lease.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return Ok(()),
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
    }
}

/// Whether `directory` holds `*.json` input.
pub fn has_json(directory: &Path) -> Result<bool> {
    if !directory.exists() {
        return Ok(false);
    }
    for entry in std::fs::read_dir(directory)? {
        if entry?.path().extension().is_some_and(|e| e == "json") {
            return Ok(true);
        }
    }
    Ok(false)
}
