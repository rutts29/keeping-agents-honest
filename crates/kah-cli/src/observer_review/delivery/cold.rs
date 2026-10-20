//! #51 (exec-loss proof run 4, 10-20): a resumed observer appends to the
//! native transcript of its old process. Read from offset 0, the old rows of
//! the same request were a receipt before the new process existed. The
//! liveness watcher then redelivered during the slow start, and the new
//! process ran the same request twice.
use super::{diagnostic, prompt_marker, screen, wait_receipt, warm};
use crate::Result;
use crate::observer_review::state::{State, Store};
use std::{
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, Instant},
};

/// The native transcript before `pane run` starts a resumed observer. Its
/// rows up to `len` belong to the old process.
pub struct History {
    device: u64,
    inode: u64,
    len: u64,
}
impl History {
    pub fn before(state: &State) -> Option<History> {
        let metadata = std::fs::metadata(state.observer.transcript.as_ref()?).ok()?;
        Some(History {
            device: metadata.dev(),
            inode: metadata.ino(),
            len: metadata.len(),
        })
    }
    /// Where this launch's rows start. A new process may name the same file
    /// by another path (the overlay's sessions symlink), so compare the file.
    fn offset(&self, path: &Path) -> u64 {
        std::fs::metadata(path)
            .ok()
            .filter(|m| m.dev() == self.device && m.ino() == self.inode && m.len() >= self.len)
            .map_or(0, |_| self.len)
    }
}

/// Cold launch is not acceptance. Native SessionStart and the persisted user
/// record prove it. The window measures a launch with no pane progress.
pub fn cold(state: &State, prompt: &str, history: Option<History>) -> Result<()> {
    let pending = state
        .pending
        .as_ref()
        .ok_or("REVIEW_PENDING_MISSING stage=delivery")?;
    let active =
        crate::observer_review::preparation::active_decision(&state.worker.session, &pending.id)?;
    let marker = prompt_marker(prompt, &pending.id, &active)?;
    let (mut started, mut seen, mut phase) = (Instant::now(), Vec::new(), None);
    loop {
        if let Err(error) = crate::observer_review::native_receipt::ensure(&state.worker.session) {
            match crate::observer_review::native_receipt::startup_progress(&mut seen, &error) {
                None => return Err(error),
                Some(true) => started = Instant::now(),
                Some(false) => (),
            }
        }
        let current =
            Store::read(&state.worker.session)?.ok_or("REVIEW_STATE_MISSING stage=delivery")?;
        if let Some(offset) = bound(&current, history.as_ref()) {
            crate::observer_review::delivery_outcome::begin(
                &current,
                marker.trim().strip_prefix("kah review request ").unwrap(),
                prompt,
                offset,
            )?;
            if wait_receipt(&current, &marker, offset)? {
                return Ok(());
            }
            diagnostic(&current, &marker, &screen(&current)?)?;
            return warm(&current, prompt);
        }
        if crate::observer_review::cold_progress::observed(&current, &mut phase) {
            started = Instant::now();
        }
        if started.elapsed() >= Duration::from_secs(15) {
            diagnostic(
                &current,
                "cold launch: native binding absent",
                &screen(&current)?,
            )?;
            return Err("REVIEW_NATIVE_UNBOUND stage=observer_binding: no verified native receipt; request retained without redelivery".into());
        }
        crate::observer_review::runtime::require_active(&state.worker.session)?;
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The receipt offset once this launch's process is bound. A resumed session
/// keeps the old session and transcript, so only the new process's
/// SessionStart (which sets the PID that binding cleared) binds it.
fn bound(current: &State, history: Option<&History>) -> Option<u64> {
    let path = current
        .observer
        .transcript
        .as_ref()
        .filter(|p| p.exists())?;
    current.observer.session.as_ref()?;
    match history {
        None => Some(0),
        Some(history) => current.observer.pid.map(|_| history.offset(path)),
    }
}
