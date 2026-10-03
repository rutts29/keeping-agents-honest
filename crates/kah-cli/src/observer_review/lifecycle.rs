//! A host process wait, not a timer or model poll. One watcher per worker PID.
use super::{control, state::Store};
use crate::Result;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn start(worker: &str) -> Result<()> {
    let config = control::config(worker)?.ok_or("REVIEW_CONFIG_MISSING stage=lifecycle")?;
    let pid = config.pid.ok_or("REVIEW_WORKER_PID stage=lifecycle")?;
    let stamp = config
        .process_stamp
        .ok_or("REVIEW_WORKER_STAMP stage=lifecycle")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(Store::directory(worker)?.join(format!("watch-{pid}-{stamp}.lock")))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(fs::TryLockError::WouldBlock) => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    let home = std::path::PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=lifecycle")?);
    Command::new(home.join(".local/bin/kah"))
        .args(["__review-watch", worker, &pid.to_string(), &stamp])
        .stdin(Stdio::from(lock))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}
pub fn watch(worker: &str, pid: u32, stamp: &str) -> Result<()> {
    wait_exit(&Store::directory(worker)?, pid, stamp)?;
    stop(worker, pid, stamp)
}
pub fn wait_exit(directory: &std::path::Path, pid: u32, stamp: &str) -> Result<()> {
    if stamp.is_empty() || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        return Err("REVIEW_PROCESS_STAMP stage=lifecycle".into());
    }
    if kah_observer::session::process_stamp(pid).as_deref() == Some(stamp) {
        let path = directory.join(format!("watch-{pid}-{stamp}.pid"));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        writeln!(file, "{pid}")?;
        file.sync_all()?;
        let result = Command::new("/usr/bin/pidwait")
            .arg("-F")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        fs::remove_file(path)?;
        // pidwait uses pidfds; an already-exited PID is a normal completion.
        if !result.success() && kah_observer::session::process_stamp(pid).as_deref() == Some(stamp)
        {
            return Err("REVIEW_PIDWAIT_FAILED stage=lifecycle".into());
        }
    }
    Ok(())
}
fn stop(worker: &str, pid: u32, stamp: &str) -> Result<()> {
    if Store::read(worker)?.is_none() {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(95);
    let store = loop {
        match Store::open(worker) {
            Ok(store) => break store,
            Err(e) if e.to_string().starts_with("REVIEW_BUSY") && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e),
        }
    };
    let Some(mut state) = store.load()? else {
        return Ok(());
    };
    let config = control::config(worker)?.ok_or("REVIEW_CONFIG_MISSING stage=lifecycle")?;
    if config.pid != Some(pid) || config.process_stamp.as_deref() != Some(stamp) {
        return Ok(());
    }
    if state.observer.pane.is_some() {
        let pane = control::owned_pane(&state)?;
        let (socket, _) = super::runtime::transport(&state)?;
        crate::run_pane::invoke_at(&["pane", "close", &pane.id], Some(socket))?;
    }
    state.observer.pane = None;
    state.stopped = true;
    store.save(&state)
}
