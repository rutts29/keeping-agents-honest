#[path = "track2_support/bundle_cost.rs"]
mod cost;
use serde_json::Value;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn retained_finish_usage_is_charged_once_across_archives_and_throttle_is_separate() {
    let raw = include_str!("../../../docs/gates/evidence/phase3-claude/b2/budget-events.jsonl");
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
