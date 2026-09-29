//! Durable aggregate usage for semantic keeper calls in one locked run.

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::semantic_resolver::ResolverMetadata;
use kah_core::run_store::RunStore;

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeeperUsageSummary {
    schema_version: u32,
    pub invocations: u64,
    pub calls: u64,
    pub elapsed_ms: u128,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

pub fn record(run: &RunStore, metadata: &ResolverMetadata) -> Result<(), String> {
    if metadata.calls == 0 {
        return Ok(());
    }
    let mut summary = load(run)?;
    summary.schema_version = 1;
    summary.invocations = summary.invocations.saturating_add(1);
    summary.calls = summary.calls.saturating_add(u64::from(metadata.calls));
    summary.elapsed_ms = summary.elapsed_ms.saturating_add(metadata.elapsed_ms);
    summary.input_tokens = summary
        .input_tokens
        .saturating_add(metadata.usage.input_tokens);
    summary.cached_input_tokens = summary
        .cached_input_tokens
        .saturating_add(metadata.usage.cached_input_tokens);
    summary.output_tokens = summary
        .output_tokens
        .saturating_add(metadata.usage.output_tokens);
    summary.reasoning_output_tokens = summary
        .reasoning_output_tokens
        .saturating_add(metadata.usage.reasoning_output_tokens);
    let directory = usage_directory(run)?;
    let bytes = serde_json::to_vec(&summary)
        .map_err(|error| format!("KEEPER_COULD_NOT_ENCODE_KEEPER_USAGE stage=keeper_metrics: could not encode keeper usage: {error}"))?;
    write_atomic_private(&directory.join("keeper-usage.json"), &bytes)
}

pub fn load(run: &RunStore) -> Result<KeeperUsageSummary, String> {
    let path = run.path().join("observations/keeper-usage.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(KeeperUsageSummary::default());
        }
        Err(error) => {
            return Err(format!(
                "KEEPER_COULD_NOT_INSPECT_KEEPER_USAGE stage=keeper_metrics: could not inspect keeper usage: {error}"
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("KEEPER_KEEPER_USAGE_STATE_IS_NOT_A_REGULAR_FILE stage=keeper_metrics: keeper usage state is not a regular file".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("KEEPER_KEEPER_USAGE_STATE_IS_ACCESSIBLE_TO_ANOTHER_USER stage=keeper_metrics: keeper usage state is accessible to another user".to_owned());
        }
    }
    let bytes = fs::read(&path).map_err(|error| format!("KEEPER_COULD_NOT_READ_KEEPER_USAGE stage=keeper_metrics: could not read keeper usage: {error}"))?;
    let summary: KeeperUsageSummary = serde_json::from_slice(&bytes)
        .map_err(|error| format!("KEEPER_INVALID_KEEPER_USAGE_STATE stage=keeper_metrics: invalid keeper usage state: {error}"))?;
    if summary.schema_version != 1 {
        return Err("KEEPER_UNSUPPORTED_KEEPER_USAGE_STATE stage=keeper_metrics: unsupported keeper usage state".to_owned());
    }
    Ok(summary)
}

fn usage_directory(run: &RunStore) -> Result<PathBuf, String> {
    let directory = run.path().join("observations");
    kah_core::run_store::ensure_private_directory(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn write_atomic_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err("KEEPER_KEEPER_USAGE_STATE_IS_NOT_A_REGULAR_FILE stage=keeper_metrics: keeper usage state is not a regular file".to_owned());
    }
    let parent = path.parent().ok_or_else(|| {
        "KEEPER_INVALID_KEEPER_USAGE_PATH stage=keeper_metrics: invalid keeper usage path"
            .to_owned()
    })?;
    let temporary = parent.join(format!(
        ".keeper-usage.tmp-{}-{}",
        std::process::id(),
        TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("KEEPER_COULD_NOT_STAGE_KEEPER_USAGE stage=keeper_metrics: could not stage keeper usage: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "KEEPER_COULD_NOT_WRITE_KEEPER_USAGE stage=keeper_metrics: could not write keeper usage: {error}"
        ));
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("KEEPER_COULD_NOT_PUBLISH_KEEPER_USAGE stage=keeper_metrics: could not publish keeper usage: {error}")
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("KEEPER_COULD_NOT_SYNC_KEEPER_USAGE stage=keeper_metrics: could not sync keeper usage: {error}"))
}
