//! Carry identity and local excludes, never executable Git configuration.
use super::{command, io_error, run, text};
use std::{
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
};

pub(super) fn copy(source: &Path, git_dir: &Path) -> Result<(), String> {
    for key in ["user.name", "user.email"] {
        let mut read = command();
        for variable in ["HOME", "XDG_CONFIG_HOME"] {
            if let Some(value) = std::env::var_os(variable) {
                read.env(variable, value);
            }
        }
        let value = read
            .env_remove("GIT_CONFIG_GLOBAL")
            .arg("-C")
            .arg(source)
            .args(["config", "--get", key])
            .output()
            .map_err(io_error)?;
        if value.status.success() {
            run(command()
                .arg("--git-dir")
                .arg(git_dir)
                .args(["config", key, &text(value)?]))?;
        }
    }
    match exclude(source) {
        Ok(bytes) => {
            fs::create_dir_all(git_dir.join("info")).map_err(io_error)?;
            fs::write(git_dir.join("info/exclude"), bytes).map_err(io_error)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(format!("RUN_EXCLUDE_READ stage=worktree: {e}")),
    }
    Ok(())
}
fn exclude(source: &Path) -> std::io::Result<Vec<u8>> {
    // Open each component without following links. Directory descriptors pin
    // parents even if a live source agent renames or replaces them concurrently.
    let open_dir = |path: &Path| {
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
    };
    let git = open_dir(&source.join(".git"))?;
    let info = open_dir(Path::new(&format!(
        "/proc/self/fd/{}/info",
        git.as_raw_fd()
    )))?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(format!("/proc/self/fd/{}/exclude", info.as_raw_fd()))?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 || meta.len() > 65536 {
        return Err(std::io::Error::other(
            "expected a bounded, unaliased regular exclude file",
        ));
    }
    let mut bytes = vec![];
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(std::io::Error::other("exclude file grew beyond bound"));
    }
    Ok(bytes)
}
