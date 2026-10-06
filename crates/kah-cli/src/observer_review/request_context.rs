//! Native user receipts bind a delayed hook to its own observer request.
use super::state::State;
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Context {
    pub request: String,
    pub preparation: bool,
}
impl Context {
    pub fn bound_to(&self, review: &str, active_decision: &str) -> bool {
        self.request == review || self.request == active_decision
    }
}
pub fn capture(state: &State, input: &Value) -> Result<Option<Context>> {
    if !matches!(
        input["hook_event_name"].as_str(),
        Some("PostToolUse" | "Stop")
    ) {
        return Ok(None);
    }
    let Some(path) = &state.observer.transcript else {
        return Ok(None);
    };
    let file = std::fs::File::open(path)?;
    read(
        BufReader::new(file),
        &state.worker.provider,
        input["turn_id"].as_str(),
    )
}
pub fn read(
    reader: impl BufRead,
    provider: &str,
    target_turn: Option<&str>,
) -> Result<Option<Context>> {
    let mut turn = None;
    let mut current = None;
    for line in reader.lines() {
        let Ok(row) = serde_json::from_str::<Value>(&line?) else {
            continue;
        };
        if row["type"] == "event_msg" && row["payload"]["type"] == "task_started" {
            turn = row["payload"]["turn_id"].as_str().map(str::to_owned);
        }
        let content = if provider == "claude" {
            if row["type"] != "user" || row["isMeta"] == true || row["isSidechain"] == true {
                continue;
            }
            &row["message"]["content"]
        } else {
            if row["type"] != "response_item"
                || row["payload"]["type"] != "message"
                || row["payload"]["role"] != "user"
            {
                continue;
            }
            &row["payload"]["content"]
        };
        if let Some(context) = content.as_str().and_then(parse).or_else(|| {
            content.as_array().and_then(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .find_map(parse)
            })
        }) {
            current = Some(context);
            if provider == "codex" && target_turn.is_some() && target_turn == turn.as_deref() {
                return Ok(current);
            }
        }
    }
    if provider == "codex" && target_turn.is_some() {
        Ok(None)
    } else {
        Ok(current)
    }
}
fn parse(text: &str) -> Option<Context> {
    let text = text.trim();
    let text = if text.starts_with("<pasted_content id=\"") {
        text.split_once("\">\n")?.1
    } else {
        text
    };
    let (request, rest) = text
        .trim_start()
        .strip_prefix("kah review request ")?
        .split_once(' ')?;
    if !request.starts_with("review-")
        || !request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    Some(Context {
        request: request.into(),
        preparation: rest.starts_with("(Preparation)"),
    })
}
