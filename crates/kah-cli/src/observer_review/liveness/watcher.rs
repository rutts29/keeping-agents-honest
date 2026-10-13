//! Persistent watcher lifetime; samples finish before a code upgrade.
use super::monitor::run;
use crate::{Result, observer_review::state::Store};

pub fn watch(worker: &str) -> Result<()> {
    loop {
        crate::helper_upgrade::between_samples(None)?;
        if let Err(error) = run(worker) {
            if super::super::provider_block::stops_watch(&error) {
                return Ok(());
            }
            super::failure::record(worker, error.as_ref())?;
        } else {
            super::failure::recovered(worker)?;
        }
        // Keep the cheap host watcher across review handoffs; otherwise a
        // publish/delivery crash while the worker is idle loses its watcher.
        match Store::read(worker) {
            Ok(state) if state.as_ref().is_none_or(|s| s.stopped) => return Ok(()),
            Err(error) => super::failure::record(worker, error.as_ref())?,
            _ => (),
        }
        // Filesystem sampling, no model polling or extra native turns.
        std::thread::sleep(std::time::Duration::from_secs(15));
    }
}
