use kah_cli::observer_review::{
    queue::Wake,
    recovery,
    state::{Purpose, Request},
};
use serde_json::json;

#[test]
fn recovery_keeps_every_trigger_and_whole_action_since_the_completed_review() {
    let whole = "Complete user words.\n".repeat(10000);
    let pending = Request {
        id: "old-pending".into(),
        worker: "worker".into(),
        purpose: Purpose::Drift,
        turn: "old-turn".into(),
        head: None,
        intent: json!({}),
        trigger: json!({"actions":[{"id":"old","text":whole}]}),
        documents: vec![],
        created_ms: 0,
    };
    let jobs = (0..324)
        .map(|i| {
            (
                std::path::PathBuf::from(format!("{i}.json")),
                Wake {
                    purpose: if i % 2 == 0 {
                        Purpose::Drift
                    } else {
                        Purpose::Steering
                    },
                    turn: format!("turn-{i}"),
                    trigger: json!({"actions":[{"id":i,"text":whole}]}),
                },
            )
        })
        .collect::<Vec<_>>();
    let state = recovery::context(Some(&pending), &jobs, &json!({"request":"last-complete"}));
    assert_eq!(state["coalesced_triggers"].as_array().unwrap().len(), 325);
    assert_eq!(state["actions"].as_array().unwrap().len(), 325);
    assert_eq!(state["actions"][324]["text"], whole);
    assert_eq!(state["recovery"]["since_review"], "last-complete");
}
