use crate::Result;
use std::{
    error::Error,
    fmt,
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
#[derive(Debug)]
struct ReportIo {
    path: PathBuf,
    stage: &'static str,
    error: io::Error,
}
impl fmt::Display for ReportIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "REVIEW_IO stage={}: {}: {}",
            self.stage,
            self.path.display(),
            self.error
        )
    }
}
impl Error for ReportIo {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}
fn failure(path: &Path, stage: &'static str, error: io::Error) -> Box<dyn Error> {
    Box::new(ReportIo {
        path: path.into(),
        stage,
        error,
    })
}
/// Open first, then inspect the descriptor: no symlinks, devices or FIFO waits.
pub fn read_report(path: &Path) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| failure(path, "report_open", e))?;
    if !file
        .metadata()
        .map_err(|e| failure(path, "report_metadata", e))?
        .is_file()
    {
        return Err(format!("REVIEW_REPORT_TYPE stage=consume: {}", path.display()).into());
    }
    let mut bytes = vec![];
    file.read_to_end(&mut bytes)
        .map_err(|e| failure(path, "report_read", e))?;
    Ok(bytes)
}
