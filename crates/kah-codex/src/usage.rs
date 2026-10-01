//! Native cumulative token reports, including consumption before a counter restart.
use serde_json::Value;
use std::io::{BufRead, Read};

pub fn tokens(mut reader: impl BufRead) -> Result<u64, String> {
    let (mut total, mut previous) = (0u64, 0u64);
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take(32 * 1024 * 1024)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("CODEX_USAGE_IO stage=run_usage: {e}"))?;
        if count == 0 {
            return Ok(total);
        }
        if line.last() != Some(&b'\n') {
            if count >= 32 * 1024 * 1024 {
                return Err("CODEX_USAGE_BOUND stage=run_usage".into());
            }
            return Ok(total); // The native writer has not finished this record.
        }
        let row: Value =
            serde_json::from_slice(&line).map_err(|_| "CODEX_USAGE_JSON stage=run_usage")?;
        if row["type"] != "event_msg" || row["payload"]["type"] != "token_count" {
            continue;
        }
        let Some(current) = row["payload"]["info"]["total_token_usage"]["total_tokens"].as_u64()
        else {
            continue;
        };
        total = total.saturating_add(if current >= previous {
            current - previous
        } else {
            current
        });
        previous = current;
    }
}
