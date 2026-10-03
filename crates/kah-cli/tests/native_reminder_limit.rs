use kah_cli::observer_review::{
    consume, reminder,
    state::{State, Store},
};
use serde_json::json;
use std::fs;
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
#[test]
fn oversized_reminder_stays_recoverable_and_produces_a_visible_gap() {
    let worker = format!("native-reminder-unit-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"claude-sonnet-5-5","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let original = "Whole words. ".repeat(1000);
    kah_cli::unattended_report::save(
        &store.directory,
        "reminder.json",
        &json!({"request":"review-large","purpose":"drift","turn":"old","text":original}),
    )
    .unwrap();
    let directory = store.directory.clone();
    drop(store);
    let delivered = consume::take(&worker, "PostToolUse", "new")
        .unwrap()
        .unwrap();
    assert!(delivered.get("hookSpecificOutput").is_none());
    assert!(
        delivered["systemMessage"]
            .as_str()
            .unwrap()
            .contains("REVIEW_REMINDER_NATIVE_LIMIT")
    );
    let retained: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("reminder.json")).unwrap()).unwrap();
    assert_eq!(retained["text"], original);
    assert!(directory.join("gap.json").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
