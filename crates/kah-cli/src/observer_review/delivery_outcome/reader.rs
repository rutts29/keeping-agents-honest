//! Complete native turns, not prose matching, drive the delivery protocol.
use super::*;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Progress {
    pub cursor: u64,
    pub delivered: Option<u64>,
    pub completed: bool,
    pub unanswered: u32,
    pub last_reply: String,
}

pub fn scan(
    provider: &str,
    session: &str,
    path: &Path,
    request: &str,
    body_digest: &str,
    progress: &mut Progress,
) -> Result<Option<Value>> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    reader.seek(SeekFrom::Start(progress.cursor))?;
    let mut bytes = vec![];
    loop {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes)? == 0 || !bytes.ends_with(b"\n") {
            return Ok(None);
        }
        let offset = progress.cursor;
        progress.cursor += bytes.len() as u64;
        if let Some(outcome) = record(
            provider,
            session,
            request,
            body_digest,
            progress,
            &bytes,
            offset,
        )? {
            return Ok(Some(outcome));
        }
    }
}

fn record(
    provider: &str,
    session: &str,
    request: &str,
    body_digest: &str,
    progress: &mut Progress,
    bytes: &[u8],
    offset: u64,
) -> Result<Option<Value>> {
    let row: Value = serde_json::from_slice(bytes)?;
    if row["isSidechain"] == true
        || row["isMeta"] == true
        || row["sessionId"].as_str().is_some_and(|id| id != session)
    {
        return Ok(None);
    }
    if let Some(text) = user(provider, &row) {
        let body = super::super::delivery_text::body(&text);
        if digest(body.as_bytes()) == body_digest {
            progress.delivered = Some(offset);
            progress.completed = false;
            progress.last_reply.clear();
        } else if !text.trim().is_empty() {
            progress.delivered = None;
            progress.unanswered = 0;
        }
    }
    if progress.delivered.is_none() || progress.completed {
        return Ok(None);
    }
    if let Some(reply) = assistant(provider, &row) {
        progress.last_reply = reply;
    }
    if !terminal(provider, &row) {
        return Ok(None);
    }
    progress.completed = true;
    progress.unanswered += 1;
    let refused = refusal(&progress.last_reply, request);
    if !refused && progress.unanswered < 2 {
        return Ok(None);
    }
    Ok(Some(
        json!({"error":if refused {"DELIVERY_REFUSED stage=delivery"} else {"DELIVERY_UNANSWERED stage=delivery"},
        "request":request,"body_digest":body_digest,"native_offset":offset,
        "delivered_offset":progress.delivered,"native_digest":digest(bytes),
        "native_record":String::from_utf8(bytes.to_vec())?,"reply":progress.last_reply,
        "unanswered":progress.unanswered,"automatic_redelivery":false}),
    ))
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned).or_else(|| {
        value.as_array().map(|parts| {
            parts
                .iter()
                .filter_map(|p| {
                    matches!(
                        p["type"].as_str(),
                        Some("text" | "input_text" | "output_text")
                    )
                    .then(|| p["text"].as_str())
                    .flatten()
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
    })
}
fn user(provider: &str, row: &Value) -> Option<String> {
    if provider == "claude" && row["type"] == "user" {
        text(&row["message"]["content"])
    } else if provider == "codex"
        && row["type"] == "response_item"
        && row["payload"]["type"] == "message"
        && row["payload"]["role"] == "user"
    {
        text(&row["payload"]["content"])
    } else {
        None
    }
}
fn assistant(provider: &str, row: &Value) -> Option<String> {
    if provider == "claude" && row["type"] == "assistant" {
        text(&row["message"]["content"])
    } else if provider == "codex"
        && row["type"] == "response_item"
        && row["payload"]["type"] == "message"
        && row["payload"]["role"] == "assistant"
    {
        text(&row["payload"]["content"])
    } else {
        None
    }
}
fn terminal(provider: &str, row: &Value) -> bool {
    if provider == "claude" {
        row["type"] == "assistant" && row["message"]["stop_reason"] == "end_turn"
    } else {
        row["type"] == "event_msg" && row["payload"]["type"] == "task_complete"
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusal {
    outcome: String,
    request: String,
    reason: String,
}
fn refusal(text: &str, request: &str) -> bool {
    serde_json::from_str::<Refusal>(text).is_ok_and(|r| {
        r.outcome == "DELIVERY_REFUSED" && r.request == request && !r.reason.trim().is_empty()
    })
}
