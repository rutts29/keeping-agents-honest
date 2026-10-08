//! Descriptor-relative Linux reads; a renamed/symlinked parent cannot redirect
//! the host outside the already-bound observer workspace. No unsafe code.
use crate::Result;
use std::{
    fs::OpenOptions,
    io::Read,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Component, Path},
};
pub(crate) fn report(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let canonical = root.canonicalize()?;
    let relative = path
        .strip_prefix(root)
        .or_else(|_| path.strip_prefix(&canonical))
        .map_err(|_| "REVIEW_EXPANSION_SCOPE stage=packet")?;
    let components = relative.components().collect::<Vec<_>>();
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&canonical)
        .map_err(|e| format!("REVIEW_EXPANSION_REPORT_IO stage=packet: {e}"))?;
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err("REVIEW_EXPANSION_SCOPE stage=packet".into());
        };
        let flags = libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if index + 1 == components.len() {
                0
            } else {
                libc::O_DIRECTORY
            };
        let relative =
            std::path::PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd())).join(name);
        file = OpenOptions::new()
            .read(true)
            .custom_flags(flags)
            .open(relative)
            .map_err(|e| format!("REVIEW_EXPANSION_REPORT_IO stage=packet: {e}"))?;
    }
    if !file.metadata()?.is_file() {
        return Err("REVIEW_EXPANSION_REPORT_TYPE stage=packet".into());
    }
    let mut bytes = vec![];
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
