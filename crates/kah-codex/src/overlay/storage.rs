use super::{OverlayError, Policy, render_config};
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, symlink},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn io_error(error: std::io::Error) -> OverlayError {
    OverlayError {
        code: "OVERLAY_IO",
        detail: error.to_string(),
    }
}

/// `directory` is a stable kah-owned project/policy directory, retained for
/// native rollout references. Only native login and history stores are shared.
pub fn prepare(original: &Path, directory: &Path, policy: &Policy<'_>) -> Result<(), OverlayError> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(io_error)?;
    if fs::symlink_metadata(directory)
        .map_err(io_error)?
        .file_type()
        .is_symlink()
    {
        return Err(OverlayError {
            code: "OVERLAY_SYMLINK",
            detail: directory.display().to_string(),
        });
    }
    let source = fs::read_to_string(original.join("config.toml")).map_err(io_error)?;
    let mut config: toml::Table =
        toml::from_str(&render_config(&source, policy)?).map_err(|e| OverlayError {
            code: "CONFIG_PARSE",
            detail: e.to_string(),
        })?;
    config.insert(
        "sqlite_home".into(),
        toml::Value::String(original.to_string_lossy().into_owned()),
    );
    let text = toml::to_string(&config).map_err(|e| OverlayError {
        code: "CONFIG_ENCODE",
        detail: e.to_string(),
    })?;
    write_private(&directory.join("config.toml"), text.as_bytes())?;
    for name in [
        "auth.json",
        "sessions",
        "archived_sessions",
        "history.jsonl",
        "session_index.jsonl",
        "models_cache.json",
    ] {
        share(&original.join(name), &directory.join(name))?;
    }
    if !policy.restricted {
        for name in ["hooks.json", "skills", "plugins", "packages"] {
            if original.join(name).exists() {
                share(&original.join(name), &directory.join(name))?;
            }
        }
    }
    Ok(())
}

fn share(source: &Path, target: &Path) -> Result<(), OverlayError> {
    match fs::symlink_metadata(target) {
        Ok(_) if fs::read_link(target).map_err(io_error)? == source => Ok(()),
        Ok(_) => Err(OverlayError {
            code: "OVERLAY_SHARED_PATH",
            detail: target.display().to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            symlink(source, target).map_err(io_error)
        }
        Err(e) => Err(io_error(e)),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), OverlayError> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.is_file()
    {
        return Err(OverlayError {
            code: "OVERLAY_CONFIG_TYPE",
            detail: path.display().to_string(),
        });
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("{}-{nonce}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // rename replaces a directory entry, never follows a changed symlink.
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(io_error)
}
