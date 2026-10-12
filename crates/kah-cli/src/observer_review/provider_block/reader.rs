use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
};

#[derive(Deserialize, Default)]
struct Row {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    payload: Payload,
    #[serde(rename = "apiError", alias = "api_error", default)]
    api_error: Option<String>,
    #[serde(rename = "isSidechain", default)]
    sidechain: bool,
    message: Option<Message>,
    timestamp: Option<String>,
}
#[derive(Deserialize, Default)]
struct Payload {
    #[serde(rename = "type", default)]
    kind: String,
    error: Option<Value>,
}
#[derive(Deserialize, Default)]
struct Message {
    stop_reason: Option<String>,
}

pub(super) fn detect(provider: &str, path: &std::path::Path) -> Result<Option<NativeBlock>> {
    Ok(scan(provider, path, 0)?.0)
}
pub(super) fn scan(
    provider: &str,
    path: &std::path::Path,
    start: u64,
) -> Result<(Option<NativeBlock>, u64)> {
    let file = File::open(path).map_err(|e| {
        format!(
            "REVIEW_PROVIDER_SOURCE_IO stage=native_provider: {}: {e}",
            path.display()
        )
    })?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start))?;
    let (mut offset, mut bytes, mut found) = (start, vec![], None);
    loop {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes)? == 0 || !bytes.ends_with(b"\n") {
            break;
        }
        let end = offset + bytes.len() as u64;
        let row: Row = serde_json::from_slice(&bytes).map_err(|e| {
            format!("REVIEW_PROVIDER_SOURCE_JSON stage=native_provider: offset={offset}: {e}")
        })?;
        if let Some(code) = policy(provider, &row) {
            found = Some(NativeBlock {
                source: path.to_owned(),
                offset,
                end,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                code: code.into(),
                timestamp: row.timestamp,
                record: String::from_utf8(bytes.clone())?,
            });
        }
        offset = end;
    }
    Ok((found, offset))
}
fn policy<'a>(provider: &str, row: &'a Row) -> Option<&'a str> {
    // openai/codex rust-v0.162.1 protocol/src/protocol.rs:1879.
    if provider == "codex"
        && row.kind == "event_msg"
        && row.payload.kind == "task_complete"
        && row
            .payload
            .error
            .as_ref()
            .is_some_and(|e| e["codex_error_info"] == "misalignment_policy_violation")
    {
        return Some("misalignment_policy_violation");
    }
    // Claude Code2.1.296 native xho predicate; binary24972e3b…: Is constructor
    // byte219977006. Official SDK schema documents these terminal policy errors.
    if provider == "claude" && row.kind == "assistant" && !row.sidechain {
        if row.message.as_ref().and_then(|m| m.stop_reason.as_deref()) == Some("refusal") {
            return Some("refusal");
        }
        if let Some(code) = row
            .api_error
            .as_deref()
            .filter(|c| matches!(*c, "dlp_request_denied" | "safety_monitor_blocked"))
        {
            return Some(code);
        }
    }
    None
}
