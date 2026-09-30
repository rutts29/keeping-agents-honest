//! Operator-owned input is separate from native exec output and judge results.
use kah_core::{
    intent_history::{SourceProvider, SourceRole},
    session_bundle::{BundleRecord, NeutralSessionBundle},
    unattended::RunControl,
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn user_bundle(
    project: &Path,
    id: &str,
    control: &RunControl,
) -> Result<NeutralSessionBundle, String> {
    let project = project
        .canonicalize()
        .map_err(|e| format!("RUN_SOURCE_PROJECT stage=intake: {e}"))?;
    let project = project
        .to_str()
        .ok_or("RUN_SOURCE_PATH stage=intake: project is not UTF-8")?;
    let digest = format!("sha256:{:x}", Sha256::digest(project.as_bytes()));
    let records = control
        .inputs()
        .iter()
        .map(|input| {
            BundleRecord::new(
                format!("user-{}", input.sequence),
                SourceRole::User,
                &input.text,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    NeutralSessionBundle::from_records(SourceProvider::Bundle, id, digest, records)
        .map_err(|e| e.to_string())
}
