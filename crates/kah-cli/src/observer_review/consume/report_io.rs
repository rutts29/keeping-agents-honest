use crate::Result;
use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, path::Path};
/// Open first, then inspect the descriptor: no symlinks, devices or FIFO waits.
pub fn read_report(path: &Path) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err("REVIEW_REPORT_TYPE stage=consume".into());
    }
    let mut bytes = vec![];
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
