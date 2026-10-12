//! Host admission returns must be durable and visible, even with no pending request.
use kah_cli::observer_review::{queue, state::Store};
use serde_json::json;
use std::fs;
#[test]
fn failure_without_a_pending_review_is_retained_and_notified_once() {
    let worker = format!("admission-failure-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    drop(store);
    for _ in 0..2 {
        let result = queue::outcome::run(&worker, || {
            Err("COMMIT_BINARY_SOURCE stage=input: native direct review required".into())
        });
        assert!(result.is_err());
    }
    let text = fs::read_to_string(root.join("admission-outcomes.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 4);
    assert!(text.contains("COMMIT_BINARY_SOURCE stage=input"));
    let notice = queue::outcome::notice(&worker).unwrap().unwrap();
    assert_eq!(
        notice.as_object().unwrap().len(),
        1,
        "native notice shape must preserve intent intake"
    );
    assert!(
        notice["systemMessage"]
            .as_str()
            .unwrap()
            .contains("queued triggers retained")
    );
    assert!(queue::outcome::notice(&worker).unwrap().is_none());
    queue::outcome::run(&worker, || {
        Ok(json!({"status":"queued","reason":"admission_inflight"}))
    })
    .unwrap();
    assert!(
        root.join("admission-blocked.json").exists(),
        "coalesced wake does not claim recovery"
    );
    queue::outcome::run(&worker, || {
        Ok(json!({"status":"submitted","request":"review-new"}))
    })
    .unwrap();
    assert!(!root.join("admission-blocked.json").exists());
    assert!(queue::outcome::notice(&worker).unwrap().is_none());
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
