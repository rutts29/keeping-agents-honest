use kah_cli::observer_review::{
    consume, reminder, reminder_queue,
    state::{State, Store},
};
use serde_json::json;
use std::{fs, path::PathBuf};
#[test]
fn native_claude_length_uses_utf16_and_never_truncates_or_caps_codex() {
    let fits = "🦀".repeat(5000);
    reminder::validate_native("claude", &fits).unwrap();
    let original = "🦀".repeat(5001);
    let unchanged = original.clone();
    assert!(
        reminder::validate_native("claude", &original)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_REMINDER_NATIVE_LIMIT")
    );
    assert_eq!(original, unchanged);
    reminder::validate_native("codex", &original).unwrap();
}
/// A Claude worker's review store.
fn claude_worker(name: &str) -> (String, PathBuf) {
    let worker = format!("{name}-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"claude-sonnet-5-5","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    (worker, store.directory.clone())
}
#[test]
fn an_oversized_claude_reminder_is_delivered_whole_and_removed() {
    // On Claude Code 2.1.305, additionalContext over
    // 10,000 characters whole to a file and gives the model its path.
    let (worker, directory) = claude_worker("native-reminder-unit");
    let original = "Whole words. ".repeat(1000);
    kah_cli::unattended_report::save(
        &directory,
        "reminder.json",
        &json!({"request":"review-large","purpose":"drift","turn":"old","text":original}),
    )
    .unwrap();
    let delivered = consume::take(&worker, "PostToolUse", "new")
        .unwrap()
        .unwrap();
    let context = delivered["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.starts_with("[kah confirmed finding from previous turn"));
    assert!(context.ends_with(&original), "text was lost");
    assert!(reminder_queue::next(&directory).unwrap().is_none());
    assert!(!directory.join("gap.json").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
#[test]
fn an_oversized_claude_reminder_does_not_block_the_next_one() {
    // An oversized reminder stayed at the queue head, so each hook
    // repeated its gap and a later reminder was never delivered.
    let (worker, directory) = claude_worker("native-reminder-order");
    let large = "a".repeat(10_001);
    let next = "Keep the public API unchanged.";
    for (request, order, text) in [
        ("review-large", "1", large.as_str()),
        ("review-next", "2", next),
    ] {
        reminder_queue::save(
            &directory,
            &json!({"request":request,"purpose":"intent","turn":"t","text":text,"delivery_order":order}),
        )
        .unwrap();
    }
    let delivered = (0..3)
        .filter_map(|_| consume::take(&worker, "PostToolUse", "t").unwrap())
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string(&delivered)
            .unwrap()
            .chars()
            .take(600)
            .collect::<String>()
    );
    assert!(
        delivered
            .iter()
            .any(|d| d["hookSpecificOutput"]["additionalContext"] == next),
        "the next reminder was never delivered: the oversized one blocks the queue head"
    );
    assert_eq!(delivered.len(), 2);
    assert_eq!(
        delivered[0]["hookSpecificOutput"]["additionalContext"],
        large
    );
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
