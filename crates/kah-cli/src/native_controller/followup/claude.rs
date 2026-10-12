//! Claude Code2.1.296 primary reply followed by durable turn_duration.
use super::*;
use serde_json::Value;

pub(super) fn progress(
    path: &Path,
    session: &str,
    receipt: &str,
) -> Result<Option<(String, String)>> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("RUN_FOLLOWUP_SOURCE_IO stage=followup_progress: {e}"))?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Ok(None);
    }
    let mut reply = None;
    let mut completed = None;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let row: Value = serde_json::from_str(line)
            .map_err(|_| "RUN_FOLLOWUP_NATIVE_JSON stage=followup_progress")?;
        if row["isSidechain"] == true || row["sessionId"].as_str().is_some_and(|id| id != session) {
            continue;
        }
        if row["type"] == "user" {
            reply = None;
        }
        if row["type"] == "assistant"
            && row["isMeta"] != true
            && row["message"]["content"].as_array().is_some_and(|items| {
                items.iter().any(|c| {
                    c["type"] == "text" && c["text"].as_str().is_some_and(|t| t.contains(receipt))
                })
            })
        {
            reply = Some(identity(&row["uuid"])?);
        }
        if row["type"] == "system"
            && row["subtype"] == "turn_duration"
            && let Some(reply) = reply.take()
        {
            completed = Some((reply, identity(&row["uuid"])?));
        }
    }
    Ok(completed)
}
