//! hook::spawn starts nothing while a job holds its lock (try_lock WouldBlock
//! returns Ok), so a job that exits while its input holds work loses that
//! wakeup until some later hook spawns it again.
use super::state::Store;
use crate::Result;
use std::{
    fs::{File, TryLockError},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

/// Jobs whose input stays true while they or the jobs they start own it, so
/// it cannot be their recheck (finding #21). They settle on spawn requests.
const REQUESTED: [&str; 4] = [
    "__review-kick",
    "__gate-host",
    "__gate-followup",
    "__review-questions",
];

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

fn marker(worker: &str, command: &str) -> Result<PathBuf> {
    Ok(Store::directory(worker)?.join(format!("job-{command}.wake")))
}
/// hook::spawn records the request before its try_lock, so the job that holds
/// the lock finds it after its pass. A job never requests itself: an
/// in-process kick from its own pass would otherwise loop.
pub fn request(worker: &str, command: &str, lock: &File) -> Result<()> {
    if !REQUESTED.contains(&command) || inherited(lock)? {
        return Ok(());
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(marker(worker, command)?)?;
    Ok(())
}
/// Take the request atomically: true when a spawn requested this job.
pub fn take_request(worker: &str, command: &str) -> Result<bool> {
    match std::fs::remove_file(marker(worker, command)?) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
/// Run a requested job. The request is cleared before the first pass reads any
/// input, so a write after that clear always causes one more pass.
pub fn until_requested(
    worker: &str,
    command: &str,
    lease: &File,
    pass: impl FnMut() -> Result<()>,
) -> Result<()> {
    take_request(worker, command)?;
    until_settled(lease, pass, || take_request(worker, command))
}
/// Whether this process is the job that holds `lock` (stdin is its lease).
fn inherited(lock: &File) -> Result<bool> {
    use std::os::fd::AsFd;
    let Ok(stdin) = std::io::stdin().as_fd().try_clone_to_owned() else {
        return Ok(false);
    };
    same(lock, &File::from(stdin))
}
pub fn same(a: &File, b: &File) -> Result<bool> {
    let (a, b) = (a.metadata()?, b.metadata()?);
    Ok(a.dev() == b.dev() && a.ino() == b.ino())
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
