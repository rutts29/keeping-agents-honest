use kah_cli::observer_review::events;
use serde_json::json;
#[test]
fn batches_keep_heavy_events_and_each_distinct_light_sha_with_original_claims() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let events = vec![
        json!({"kind":"native_head","review_class":"commit","head":a}),
        json!({"kind":"native_phase_claim","review_class":"commit","head":a,"source":"whole original claim"}),
        json!({"kind":"native_head","review_class":"commit","head":b}),
        json!({"kind":"native_git_tag","review_class":"commit","tag":"gate"}),
    ];
    let batches = events::batches(&events);
    assert_eq!(batches.len(), 3);
    let heavy = batches
        .iter()
        .find(|b| b["review_class"] == "gate")
        .unwrap();
    assert_eq!(heavy["events"][0]["kind"], "native_git_tag");
    let first = batches.iter().find(|v| v["head"] == a).unwrap();
    assert_eq!(first["events"].as_array().unwrap().len(), 2);
    assert_eq!(first["events"][1]["source"], "whole original claim");
    assert!(batches.iter().any(|v| v["head"] == b));
}
