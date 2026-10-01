//! Exact private completion replay, with no run-control or ledger mutation.
use crate::completion_capture::{self, Request};
use kah_observer::judge;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub fn run(home: &Path, path: &Path) -> Result<PathBuf, String> {
    let saved = load(path)?;
    let session = saved["session"]
        .as_str()
        .ok_or("COMPLETION_REPLAY_SESSION stage=replay")?;
    let commit = saved["commit"]
        .as_str()
        .ok_or("COMPLETION_REPLAY_COMMIT stage=replay")?;
    let body = &saved["request"];
    let questions = serde_json::from_value(body["questions"].clone())
        .map_err(|_| "COMPLETION_REPLAY_QUESTIONS stage=replay")?;
    let request = Request::new(
        body["model"]
            .as_str()
            .ok_or("COMPLETION_REPLAY_MODEL stage=replay")?,
        &body["state"],
        &questions,
    )?;
    if request.body() != *body
        || saved["request_digest"] != format!("{:x}", Sha256::digest(body.to_string().as_bytes()))
    {
        return Err(
            "COMPLETION_REPLAY_INTEGRITY stage=replay: request changed or is not scrubbed".into(),
        );
    }
    let result = judge::routing::evaluate(
        home,
        judge::routing::Backend::Jev,
        request.model(),
        request.state(),
        request.questions(),
    );
    let output = completion_capture::save(home, session, commit, "replay", &request, &result)?;
    result.map_err(|e| {
        format!(
            "COMPLETION_REPLAY_JUDGE stage=replay: {e}; saved={}",
            output.display()
        )
    })?;
    Ok(output)
}
fn load(path: &Path) -> Result<Value, String> {
    let meta = fs::symlink_metadata(path).map_err(io_error)?;
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 || meta.len() > 2 * 1024 * 1024 {
        return Err("COMPLETION_REPLAY_PRIVATE stage=replay: private regular file required".into());
    }
    let mut bytes = vec![];
    fs::File::open(path)
        .and_then(|file| file.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(io_error)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("COMPLETION_REPLAY_BOUND stage=replay".into());
    }
    let saved: Value =
        serde_json::from_slice(&bytes).map_err(|_| "COMPLETION_REPLAY_JSON stage=replay")?;
    if saved["schema"] != 1 {
        return Err("COMPLETION_REPLAY_SCHEMA stage=replay".into());
    }
    Ok(saved)
}
fn io_error(error: std::io::Error) -> String {
    format!("COMPLETION_REPLAY_IO stage=replay: {error}")
}
