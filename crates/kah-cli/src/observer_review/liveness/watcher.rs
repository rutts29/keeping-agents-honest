//! Persistent watcher lifetime; samples finish before a code upgrade.
use super::monitor::run;
use crate::{
    Result,
    observer_review::{control, lifecycle, state::Store},
};

pub fn watch(worker: &str) -> Result<()> {
    loop {
        crate::helper_upgrade::between_samples(None)?;
        // The worker process is the session. Its exit ends supervision even
        // when no lifecycle watcher started, e.g. after a failed cold delivery.
        if let Some((pid, stamp)) = ended_worker(control::config(worker)?.as_ref()) {
            return lifecycle::stop(worker, pid, &stamp);
        }
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

/// The registered worker PID and stamp, once that exact process has exited.
pub(crate) fn ended_worker(config: Option<&control::Config>) -> Option<(u32, String)> {
    let config = config?;
    let (pid, stamp) = (config.pid?, config.process_stamp.clone()?);
    (kah_observer::session::process_stamp(pid).as_deref() != Some(stamp.as_str()))
        .then_some((pid, stamp))
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/liveness_worker_exit.rs"]
mod tests;
