use kah_observer::tool_watch::Progress;
use serde_json::json;

#[test]
fn checks_require_four_distinct_completed_tools_and_repeat_delivery_does_not_advance() {
    let mut progress = Progress::default();
    for id in ["a", "b", "c"] {
        assert!(
            progress
                .push("turn", id, json!({"id":id}))
                .unwrap()
                .is_none()
        );
    }
    assert!(
        progress
            .push("turn", "a", json!({"id":"a"}))
            .unwrap()
            .is_none()
    );
    let batch = progress
        .push("turn", "d", json!({"id":"d"}))
        .unwrap()
        .unwrap();
    assert_eq!(batch.len(), 4);
    assert_eq!(batch[0]["id"], "a");
    let mut restored: Progress =
        serde_json::from_value(serde_json::to_value(progress).unwrap()).unwrap();
    assert!(
        restored
            .push("turn", "d", json!({"id":"d"}))
            .unwrap()
            .is_none()
    );
    assert!(
        restored
            .push("next-turn", "a", json!({"id":"a"}))
            .unwrap()
            .is_none()
    );
    for id in ["e", "f"] {
        assert!(
            restored
                .push("next-turn", id, json!({"id":id}))
                .unwrap()
                .is_none()
        );
    }
    assert!(
        restored
            .push("next-turn", "g", json!({"id":"g"}))
            .unwrap()
            .is_some()
    );
    assert!(restored.push("turn", "", json!({})).is_err());
}

#[test]
fn tool_window_and_identity_memory_stay_bounded() {
    let mut progress = Progress::default();
    let mut checks = 0;
    for n in 0..1000 {
        if progress
            .push("long-turn", &n.to_string(), json!({"id":n}))
            .unwrap()
            .is_some()
        {
            checks += 1;
        }
    }
    assert_eq!(checks, 250);
    assert!(serde_json::to_vec(&progress).unwrap().len() < 4096);
}
