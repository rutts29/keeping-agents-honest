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
/// A worker's review store.
fn claude_worker(name: &str) -> (String, PathBuf) {
    worker(name, "claude")
}
fn worker(name: &str, provider: &str) -> (String, PathBuf) {
    let worker = format!("{name}-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":provider,"session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"claude-sonnet-5-5","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    (worker, store.directory.clone())
}
#[test]
fn an_oversized_claude_reminder_is_delivered_whole_and_removed() {
    // The drift label is added once, before the first part.
    let (worker, directory) = claude_worker("native-reminder-unit");
    let original = "Whole words. ".repeat(1000);
    kah_cli::unattended_report::save(
        &directory,
        "reminder.json",
        &json!({"request":"review-large","purpose":"drift","turn":"old","text":original}),
    )
    .unwrap();
    let delivered = std::iter::from_fn(|| consume::take(&worker, "PostToolUse", "new").unwrap())
        .take(20)
        .map(|d| {
            d["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(delivered.len(), 2);
    let whole = joined(&delivered);
    assert!(whole.starts_with("[kah confirmed finding from previous turn"));
    assert_eq!(whole.matches("[kah confirmed finding").count(), 1);
    assert!(whole.ends_with(&original), "text was lost");
    assert!(reminder_queue::next(&directory).unwrap().is_none());
    assert!(!directory.join("gap.json").exists());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
#[test]
fn an_oversized_claude_reminder_does_not_block_the_next_one() {
    // The oversized reminder stayed at the queue head, so each hook
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
    let delivered = drain(&worker);
    let (last, parts) = delivered.split_last().unwrap();
    assert_eq!(
        last, next,
        "the next reminder was never delivered: the oversized one blocks the queue head"
    );
    // No line end: the parts split at a character boundary.
    assert_eq!(joined(parts), large);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
/// The parts' text in order, without their markers.
fn joined(parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| {
            let (marker, body) = p.split_once('\n').unwrap();
            assert!(marker.starts_with("[kah message part "), "{marker}");
            body
        })
        .collect()
}
/// Whole lines with 2-unit characters, padded to exactly `units` UTF-16 units.
fn long_text(units: usize) -> String {
    let mut text = String::new();
    let mut line = 0;
    while text.encode_utf16().count() + 40 < units {
        text.push_str(&format!("Line {line}: whole words 🦀 stay whole.\n"));
        line += 1;
    }
    let pad = units - text.encode_utf16().count();
    text + &"a".repeat(pad)
}
/// Every hook string `take` delivers until the queue is empty.
fn drain(worker: &str) -> Vec<String> {
    std::iter::from_fn(|| consume::take(worker, "PostToolUse", "t").unwrap())
        .take(20)
        .map(|d| {
            d["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap_or_else(|| panic!("no context: {d}"))
                .to_owned()
        })
        .collect()
}
#[test]
fn a_long_claude_reminder_never_relies_on_a_file_the_worker_cannot_read() {
    // For oversized reminders Claude saves the hook text to
    // <config root>/projects/<project>/<session>/tool-results/ and gives the
    // model a ~2,000-character preview and the path.
    let home = std::env::temp_dir().join(format!("kah-reminder-denied-{}", std::process::id()));
    let root = kah_claude::paths::config_root(&home);
    if root.starts_with(&home) {
        fs::create_dir_all(root.join("projects/p/s/tool-results")).unwrap();
    }
    let project = home.join("repos/mine/p");
    fs::create_dir_all(&project).unwrap();
    let policy =
        kah_cli::claude_policy::permissions(&home, &project, &home.join("private"), false, None)
            .unwrap();
    let projects = root.join("projects");
    let denied = format!("Read(/{}/**)", projects.display());
    assert!(policy["deny"].as_array().unwrap().contains(&denied.into()));
    fs::remove_dir_all(&home).unwrap();
    // So a delivered string over the limit would reach the model as a preview only.
    let (worker, directory) = claude_worker("native-reminder-denied");
    reminder_queue::save(
        &directory,
        &json!({"request":"review-long","purpose":"intent","turn":"t","text":long_text(25_000)}),
    )
    .unwrap();
    for context in drain(&worker) {
        let units = context.encode_utf16().count();
        assert!(
            units <= 10_000,
            "delivered {units} UTF-16 units: Claude moves all but a preview to {}, which kah denies to the worker",
            projects.display()
        );
    }
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
#[test]
fn a_long_claude_reminder_arrives_whole_in_ordered_parts() {
    let (worker, directory) = claude_worker("native-reminder-parts");
    let original = long_text(25_000);
    assert_eq!(original.encode_utf16().count(), 25_000);
    let next = "Keep the public API unchanged.";
    for (request, order, text) in [
        ("review-long", "1", original.as_str()),
        ("review-next", "2", next),
    ] {
        reminder_queue::save(
            &directory,
            &json!({"request":request,"purpose":"intent","turn":"t","text":text,"delivery_order":order}),
        )
        .unwrap();
    }
    let delivered = drain(&worker);
    let sizes = delivered
        .iter()
        .map(|c| c.encode_utf16().count())
        .collect::<Vec<_>>();
    println!("delivered sizes (UTF-16 units): {sizes:?}");
    assert!(
        sizes.iter().all(|&n| n <= 10_000),
        "a part is over 10,000: {sizes:?}"
    );
    let (last, parts) = delivered.split_last().unwrap();
    assert_eq!(last, next, "the next reminder must follow the last part");
    let (_, early) = parts.split_last().unwrap();
    assert!(
        early.iter().all(|p| p.ends_with('\n')),
        "a part ends mid-line"
    );
    assert_eq!(
        joined(parts),
        original,
        "the parts in order must be the original text"
    );
    assert!(reminder_queue::next(&directory).unwrap().is_none());
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
#[test]
fn a_long_codex_reminder_still_goes_whole() {
    let (worker, directory) = worker("native-reminder-codex", "codex");
    let original = long_text(25_000);
    reminder_queue::save(
        &directory,
        &json!({"request":"review-long","purpose":"intent","turn":"t","text":original}),
    )
    .unwrap();
    assert_eq!(drain(&worker), [original]);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
