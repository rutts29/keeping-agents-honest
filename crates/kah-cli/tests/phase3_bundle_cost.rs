#[path = "track2_support/bundle_cost.rs"]
mod cost;
use serde_json::{Value, json};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn finish_usage_is_charged_once_across_archives_and_throttle_is_separate() {
    let finish = |request: &str, ticket: &str, status: u64, tokens: Option<u64>| {
        json!({"at_ms":1_700_000_000_000_u64,"kind":"finish","request":request,"ticket":ticket,
            "status":status,"exact_input_tokens":tokens})
    };
    let raw = [
        finish("batch-1", "1", 429, None),
        finish("batch-2", "2", 200, Some(4321)),
    ]
    .iter()
    .map(|row| format!("{row}\n"))
    .collect::<String>();
    let raw = raw.as_str();
    let home = std::env::temp_dir().join(format!(
        "kah-bundle-cost-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let live = home.join(".local/state/kah/batch");
    let archive = home.join(".local/share/kah-eval/archive");
    fs::create_dir_all(&live).unwrap();
    fs::create_dir_all(&archive).unwrap();
    fs::write(live.join("events.jsonl"), format!("{raw}{{\"kind\":")).unwrap();
    fs::write(archive.join("events.jsonl"), raw).unwrap();
    let tokens = raw
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .map(|r| r["exact_input_tokens"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let result = cost::sample(&home);
    let _ = cost::stop(&home, &home);
    assert_eq!(result["exact_input_tokens"], tokens);
    assert_eq!(result["finished_attempts"], 2);
    assert_eq!(result["statuses"]["429"], 1);
    assert_eq!(result["statuses"]["200"], 1);
    assert_eq!(result["missing_success_usage"], 0);
    assert!(
        (result["actual_jev_usd"].as_f64().unwrap() - tokens as f64 * 0.042 / 1_000_000.0).abs()
            < 1e-12
    );
    fs::remove_dir_all(home).unwrap();
}
