use super::State;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub struct Store {
    pub(super) path: PathBuf,
    _lock: File,
}
impl Store {
    pub fn directory(base: &Path, session: &str) -> PathBuf {
        let digest = format!("{:x}", Sha256::digest(session.as_bytes()));
        base.join("observer-sessions").join(&digest[..24])
    }
    pub fn open(base: &Path, session: &str, wait: Duration) -> Result<Self, String> {
        Self::open_lock(base, session, wait, "lock")
    }
    pub fn open_wait(base: &Path, session: &str) -> Result<Self, String> {
        Self::locked(base, session, None, "lock")
    }
    pub(super) fn open_lock(
        base: &Path,
        session: &str,
        wait: Duration,
        name: &str,
    ) -> Result<Self, String> {
        Self::locked(base, session, Some(wait), name)
    }
    fn locked(
        base: &Path,
        session: &str,
        wait: Option<Duration>,
        name: &str,
    ) -> Result<Self, String> {
        if session.is_empty() || session.len() > 512 || session.chars().any(char::is_control) {
            return Err("OBSERVER_SESSION_ID stage=session_store: invalid session identity".into());
        }
        let path = Self::directory(base, session);
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .map_err(io_error)?;
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("OBSERVER_STATE_PRIVATE stage=session_store: state directory must be private and not a symlink".into());
        }
        let lock_path = path.join(name);
        reject_symlink(&lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(lock_path)
            .map_err(io_error)?;
        if wait.is_none() {
            lock.lock().map_err(io_error)?;
            return Ok(Self { path, _lock: lock });
        }
        let deadline = Instant::now() + wait.unwrap();
        loop {
            match lock.try_lock() {
                Ok(()) => return Ok(Self { path, _lock: lock }),
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25))
                }
                Err(fs::TryLockError::WouldBlock) => {
                    return Err(
                        "OBSERVER_BUSY stage=session_store: another observation is running".into(),
                    );
                }
                Err(fs::TryLockError::Error(e)) => return Err(io_error(e)),
            }
        }
    }
    pub fn load(&self) -> Result<Option<State>, String> {
        read_state(&self.path.join("state.json"))
    }
    pub fn read(base: &Path, session: &str) -> Result<Option<State>, String> {
        read_state(&Self::directory(base, session).join("state.json"))
    }
    pub fn save(&mut self, state: &State) -> Result<(), String> {
        let path = self.path.join("state.json");
        reject_symlink(&path)?;
        let temporary = self.path.join(format!("state-{}.tmp", std::process::id()));
        reject_symlink(&temporary)?;
        let bytes = serde_json::to_vec(state)
            .map_err(|e| format!("OBSERVER_STATE_JSON stage=session_store: {e}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(io_error)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(io_error)?;
        fs::rename(temporary, path).map_err(io_error)
    }
    pub fn append(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let path = self.path.join("observations.jsonl");
        reject_symlink(&path)?;
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(path)
            .map_err(io_error)?;
        let mut bytes =
            serde_json::to_vec(value).map_err(|_| "OBSERVER_SOURCE_JSON stage=session_store")?;
        bytes.push(b'\n');
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(io_error)
    }
}

fn read_state(path: &Path) -> Result<Option<State>, String> {
    reject_symlink(path)?;
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("OBSERVER_STATE_JSON stage=session_store: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(e)),
    }
}
fn reject_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(
            "OBSERVER_STATE_SYMLINK stage=session_store: refusing a state-file symlink".into(),
        );
    }
    Ok(())
}
fn io_error(error: std::io::Error) -> String {
    format!("OBSERVER_STATE_IO stage=session_store: {error}")
}
