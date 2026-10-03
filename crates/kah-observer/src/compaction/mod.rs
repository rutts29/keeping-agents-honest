//! Legacy summary decoding and model-choice compatibility only. Automatic
//! synthesis was retired in Phase 5O; full native sessions own compaction.
use kah_core::projection::{ActiveProjection, CompactedIntent};
use sha2::{Digest, Sha256};
mod config;
pub use config::*;

pub fn digest(projection: &ActiveProjection) -> String {
    let mut raw = projection.clone();
    raw.compaction = None;
    format!(
        "sha256:{:x}",
        Sha256::digest(raw.rendered_prompt().as_bytes())
    )
}

/// Decode historical summary artifacts; the live engine never attaches them.
pub fn attach(projection: &mut ActiveProjection, cached: Option<&CompactedIntent>) {
    projection.compaction = cached
        .filter(|c| c.source_digest == digest(projection))
        .cloned();
}
