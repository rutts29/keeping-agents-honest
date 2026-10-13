//! Bind host validation to exact submitted bytes, independently of later rewrites.
use super::{consume, source_store};
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
#[derive(Clone, Serialize, Deserialize)]
pub struct Submission {
    pub path: PathBuf,
    pub digest: String,
    pub source: PathBuf,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
impl Submission {
    pub fn capture(root: &Path, path: &Path, bytes: &[u8]) -> Result<Self> {
        let digest = digest(bytes);
        let source = root.join("report-revisions").join(format!("{digest}.txt"));
        source_store::text(root, &source, std::str::from_utf8(bytes)?)?;
        Ok(Self {
            path: path.into(),
            digest,
            source,
        })
    }
    pub fn require_current(&self) -> Result<()> {
        let latest = digest(&consume::read_report(&self.path)?);
        if latest != self.digest {
            return Err(format!(
                "REVIEW_REPORT_REVISION_SUPERSEDED stage=handoff: {} superseded by {latest}",
                self.digest
            )
            .into());
        }
        Ok(())
    }
}
