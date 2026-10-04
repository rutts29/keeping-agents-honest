use kah_cli::observer_review::{
    consume, reminder_queue,
    state::{State, Store},
};
use serde_json::json;
#[test]
fn a_running_review_cannot_suppress_an_already_confirmed_native_reminder() {
    let worker = format!("delivery-lock-unit-{}", std::process::id());
    let review = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":"/tmp","pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":"/tmp","model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    review.save(&state).unwrap();
    reminder_queue::save(&review.directory,&json!({"request":"review-confirmed","purpose":"checkpoint","turn":"t","text":"Checkpoint on exact SHA needs integration"})).unwrap();
    let delivered = consume::take(&worker, "PostToolUse", "t").unwrap().unwrap();
    assert_eq!(
        delivered["hookSpecificOutput"]["additionalContext"],
        "Checkpoint on exact SHA needs integration"
    );
    let root = review.directory.parent().unwrap().to_path_buf();
    drop(review);
    std::fs::remove_dir_all(root).unwrap();
}
