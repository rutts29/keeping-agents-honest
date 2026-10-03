use kah_cli::observer_review::pause::notice;
use serde_json::json;
use std::fs;

#[test]
fn pause_and_resume_notices_occur_once_without_dropping_hook_context() {
    let root = std::env::temp_dir().join(format!("kah-pause-notice-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let marker = root.join("observer-paused");
    assert_eq!(notice(&root, &marker).unwrap(), None);
    fs::write(&marker, "operator pause").unwrap();
    assert_eq!(notice(&root, &marker).unwrap(), Some(true));
    assert_eq!(notice(&root, &marker).unwrap(), None);
    fs::remove_file(&marker).unwrap();
    assert_eq!(notice(&root, &marker).unwrap(), Some(false));
    assert_eq!(notice(&root, &marker).unwrap(), None);
    fs::write(&marker, "new pause").unwrap();
    assert_eq!(notice(&root, &marker).unwrap(), Some(true));
    let mut output = json!({"hookSpecificOutput":{"additionalContext":"keep exact user intent"},"systemMessage":"existing note"});
    kah_cli::observer_review::pause::append(&mut output, false);
    assert_eq!(
        output["hookSpecificOutput"]["additionalContext"],
        "keep exact user intent"
    );
    assert!(
        output["systemMessage"]
            .as_str()
            .unwrap()
            .contains("existing note")
    );
    assert!(
        output["systemMessage"]
            .as_str()
            .unwrap()
            .contains("resumed")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_pause_gaps_are_quiet_but_real_errors_still_surface_once() {
    use kah_cli::observer_review::{consume, state::Store};
    let worker = format!("pause-gap-test-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    kah_cli::unattended_report::save(
        &directory,
        "gap.json",
        &json!({"error":"REVIEW_PAUSED stage=dispatch"}),
    )
    .unwrap();
    drop(store);
    assert!(
        consume::take(&worker, "PostToolUse", "t")
            .unwrap()
            .is_none()
    );
    kah_cli::unattended_report::save(
        &directory,
        "gap.json",
        &json!({"error":"REVIEW_REPORT_MISSING stage=observer"}),
    )
    .unwrap();
    assert!(
        consume::take(&worker, "PostToolUse", "t")
            .unwrap()
            .is_some()
    );
    assert!(
        consume::take(&worker, "PostToolUse", "t")
            .unwrap()
            .is_none()
    );
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
