//! Restricted embedded sessions require a gVisor-only native default.
use super::OverlayError;
use std::path::Path;
use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt};

pub fn validate_executor(directory: &Path, expected: &str) -> Result<(), OverlayError> {
    let refusal = || OverlayError {
        code: "SESSION_EXECUTOR_POLICY",
        detail: "restricted gVisor executor is missing or changed; reopen through kah".into(),
    };
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(directory.join("environments.toml"))
        .map_err(|_| refusal())?;
    let meta = file.metadata().map_err(|_| refusal())?;
    if !meta.is_file() || meta.len() > 128 * 1024 {
        return Err(refusal());
    }
    let mut text = String::new();
    file.take(128 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| refusal())?;
    let config: toml::Value = toml::from_str(&text).map_err(|_| refusal())?;
    let entry = config
        .get("environments")
        .and_then(toml::Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|e| e.get("id").and_then(toml::Value::as_str) == Some(expected))
        });
    let gvisor = entry.is_some_and(|e| {
        e.get("program").and_then(toml::Value::as_str) == Some("/usr/bin/docker")
            && e.get("args")
                .and_then(toml::Value::as_array)
                .is_some_and(|args| {
                    args.windows(2).any(|pair| {
                        pair[0].as_str() == Some("--runtime") && pair[1].as_str() == Some("runsc")
                    })
                })
    });
    if text.len() > 128 * 1024
        || config.get("default").and_then(toml::Value::as_str) != Some(expected)
        || config.get("include_local").and_then(toml::Value::as_bool) != Some(false)
        || !gvisor
    {
        return Err(refusal());
    }
    Ok(())
}
