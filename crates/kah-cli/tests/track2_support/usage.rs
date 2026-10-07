use serde_json::{Value, json};
use std::{collections::BTreeMap, io::BufRead};
pub fn parse(mut reader: impl BufRead, provider: &str) -> Value {
    let mut messages = BTreeMap::new();
    let mut totals = json!({"input":0,"cached_input":0,"cache_creation":0,"output":0});
    let mut previous = totals.clone();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 || !line.ends_with('\n') {
            break;
        }
        let row: Value = serde_json::from_str(&line).expect("TRACK2_USAGE_JSON stage=measurement");
        let usage = &row["payload"]["info"]["total_token_usage"];
        if provider == "codex" && row["payload"]["type"] == "token_count" && usage.is_object() {
            for (to, from) in [
                ("input", "input_tokens"),
                ("cached_input", "cached_input_tokens"),
                ("output", "output_tokens"),
            ] {
                let current = usage[from].as_u64().unwrap_or(0);
                let prior = previous[to].as_u64().unwrap();
                totals[to] = (totals[to].as_u64().unwrap()
                    + if current >= prior {
                        current - prior
                    } else {
                        current
                    })
                .into();
                previous[to] = current.into();
            }
        } else if provider == "claude"
            && row["type"] == "assistant"
            && row["message"]["usage"].is_object()
        {
            let id = row["message"]["id"].as_str().unwrap().to_owned();
            let usage = &row["message"]["usage"];
            let before = messages.entry(id).or_insert(json!({}));
            if charge(usage) >= charge(before) {
                *before = usage.clone();
            }
        }
    }
    for usage in messages.values() {
        for (to, from) in [
            ("input", "input_tokens"),
            ("cached_input", "cache_read_input_tokens"),
            ("cache_creation", "cache_creation_input_tokens"),
            ("output", "output_tokens"),
        ] {
            totals[to] = (totals[to].as_u64().unwrap() + usage[from].as_u64().unwrap_or(0)).into();
        }
    }
    totals
}
fn charge(usage: &Value) -> u64 {
    [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .map(|key| usage[*key].as_u64().unwrap_or(0))
    .sum()
}
