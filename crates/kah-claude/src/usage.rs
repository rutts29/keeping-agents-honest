//! One charge per native assistant message; streamed revisions update that charge.
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{BufRead, Read},
};

pub fn tokens(mut reader: impl BufRead) -> Result<u64, String> {
    let mut messages = BTreeMap::<String, u64>::new();
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take(32 * 1024 * 1024)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("CLAUDE_USAGE_IO stage=run_usage: {e}"))?;
        if count == 0 {
            break;
        }
        if line.last() != Some(&b'\n') {
            if count >= 32 * 1024 * 1024 {
                return Err("CLAUDE_USAGE_BOUND stage=run_usage".into());
            }
            break;
        }
        let row: Value =
            serde_json::from_slice(&line).map_err(|_| "CLAUDE_USAGE_JSON stage=run_usage")?;
        if row["type"] != "assistant" || !row["message"]["usage"].is_object() {
            continue;
        }
        let id = row["message"]["id"]
            .as_str()
            .ok_or("CLAUDE_USAGE_ID stage=run_usage")?;
        let usage = &row["message"]["usage"];
        let charge = [
            "input_tokens",
            "output_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ]
        .iter()
        .fold(0u64, |sum, key| {
            sum.saturating_add(usage[key].as_u64().unwrap_or(0))
        });
        let previous = messages.entry(id.into()).or_default();
        *previous = (*previous).max(charge);
    }
    Ok(messages
        .values()
        .fold(0u64, |sum, value| sum.saturating_add(*value)))
}
