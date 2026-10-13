//! Installed host helpers upgrade only between durable samples, never mid-write.
use crate::Result;
use std::{
    fs::File,
    os::{
        fd::AsFd,
        unix::{fs::MetadataExt, process::CommandExt},
    },
    path::Path,
    process::{Command, Stdio},
};

pub fn between_samples(lease: Option<&File>) -> Result<()> {
    if !std::env::args_os()
        .nth(1)
        .as_deref()
        .is_some_and(long_lived)
    {
        return Ok(()); // Library callers and finite transactions are not daemons.
    }
    let home = std::env::var_os("HOME").ok_or("HELPER_HOME stage=upgrade")?;
    let installed = Path::new(&home).join(".local/bin/kah");
    let target = std::fs::metadata(&installed).map_err(|e| {
        format!(
            "HELPER_INSTALL_READ stage=upgrade: {}: {e}",
            installed.display()
        )
    })?;
    // Linux procfs follows the running inode even after an atomic replacement.
    let running = std::fs::metadata("/proc/self/exe")?;
    if running.dev() == target.dev() && running.ino() == target.ino() {
        return Ok(());
    }
    let mut command = Command::new(&installed);
    command.args(std::env::args_os().skip(1));
    if let Some(lease) = lease {
        command.stdin(Stdio::from(lease.try_clone()?));
    }
    // stdin already owns the singleton for spawned review watchers. Exec keeps
    // PID, arguments, environment and that open description; no new daemon.
    eprintln!(
        "kah helper re-exec: pid={} installed={}",
        std::process::id(),
        installed.display()
    );
    Err(format!(
        "HELPER_REEXEC stage=upgrade: {}: {}",
        installed.display(),
        command.exec()
    )
    .into())
}

pub fn long_lived(command: &std::ffi::OsStr) -> bool {
    matches!(
        command.to_str(),
        Some(
            "__review-liveness-watch"
                | "__review-watch"
                | "__attach-observer"
                | "__untrusted-commits"
        )
    )
}

pub fn open_lease(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let expected = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    let Ok(stdin) = std::io::stdin().as_fd().try_clone_to_owned() else {
        return Ok(expected);
    };
    let inherited = File::from(stdin);
    let a = expected.metadata()?;
    let b = inherited.metadata()?;
    if a.dev() == b.dev() && a.ino() == b.ino() {
        Ok(inherited)
    } else {
        Ok(expected)
    }
}
