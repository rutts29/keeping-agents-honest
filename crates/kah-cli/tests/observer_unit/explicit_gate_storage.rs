use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn retained_claude_checkpoint_report_creates_its_typed_snapshot_directory() {
    let raw = include_str!(
        "../../../../docs/gates/evidence/phase3-claude/b2/consume-product-failure/native-checkpoint-report.json"
    );
    let report: super::super::protocol::Report = serde_json::from_str(raw).unwrap();
    let super::super::protocol::Answer::Checkpoint { rows } = report.result else {
        panic!("retained failure must be the native cumulative checkpoint report");
    };
    let cumulative = Request {
        id: report.request,
        worker: "00000000-0000-4000-8000-100000000011".into(),
        purpose: Purpose::Checkpoint,
        turn: "00000000-0000-4000-8000-100000000010".into(),
        head: Some("c835c9e3f1d8cc39b13c997bcaec3f7db7705abc".into()),
        intent: json!({"revision":2}),
        trigger: json!({"option1":{"original_trigger":{"kind":"user_gate","source":"host_cli"}}}),
        documents: vec![Document::new(
            "native-report",
            "retained_native_checkpoint",
            "kah:review-1000000000011-1",
            raw,
        )],
        created_ms: 1791755078945,
    };
    let mut captured = cumulative.clone();
    captured.id.push_str("-typed");
    let next = prepare(&cumulative, captured, &rows).unwrap();
    let base =
        std::env::temp_dir().join(format!("kah-gate-storage-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&base.join("sources")).unwrap();
    let directory = base.join("explicit-gate-inputs");
    assert!(
        !directory.exists(),
        "real failure had no snapshot directory"
    );
    save_snapshot(&directory, &cumulative, &next).unwrap();
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let stored: Request =
        super::super::super::source_store::read(&directory.join(format!("{}.json", cumulative.id)))
            .unwrap();
    assert_eq!(stored.head, cumulative.head);
    assert_eq!(stored.worker, cumulative.worker);
    assert_eq!(
        stored.documents[0].text, raw,
        "retain every native report byte"
    );
    assert_eq!(
        serde_json::to_value(stored).unwrap(),
        serde_json::to_value(&next).unwrap()
    );
    save_snapshot(&directory, &cumulative, &next).unwrap();
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn captured_055_scope_survives_live_910_head_without_mixing_code_intent_or_docs() {
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-explicit-scope/timeline.json"
    ))
    .unwrap();
    let cumulative = scope_request(&value, "before", "captured_head");
    let mut fresh = scope_request(&value, "after", "new_head");
    fresh.id = "review-new-typed".into();
    let next = prepare(&cumulative, fresh, &[]).unwrap();
    assert_eq!(next.head, cumulative.head);
    assert_eq!(next.intent, cumulative.intent);
    assert_eq!(
        serde_json::to_value(&next.documents[..cumulative.documents.len()]).unwrap(),
        serde_json::to_value(&cumulative.documents).unwrap()
    );
    assert_eq!(next.worker, cumulative.worker);
    assert_eq!(next.id, "review-new-typed");
    assert_ne!(next.head, value["new_head"].as_str().map(str::to_owned));
    assert!(next.trigger["option1"].is_null());
    assert_eq!(
        next.trigger["option1_cumulative_input"]["intent_revision"],
        90
    );
}
fn scope_request(value: &Value, body: &str, head: &str) -> Request {
    Request {
        id: "review-1700000000000-1".into(),
        worker: value["worker"].as_str().unwrap().into(),
        purpose: Purpose::Checkpoint,
        turn: "scope-replay".into(),
        head: value[head].as_str().map(str::to_owned),
        // Explicit fixture intent: this tests whole-object identity, not a
        // synthesized copy of the live History or new user authority.
        intent: json!({"revision":90,"goal":"Retain the captured scope","items":[]}),
        trigger: json!({"option1":{"original_trigger":{"kind":"native_phase_claim","review_class":"gate"}}}),
        documents: value["documents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                Document::new(
                    d["path"].as_str().unwrap(),
                    "committed_source",
                    &format!(
                        "git:{}:{}",
                        value[head].as_str().unwrap(),
                        d["path"].as_str().unwrap()
                    ),
                    d[body].as_str().unwrap(),
                )
            })
            .collect(),
        created_ms: 1791850695198,
    }
}
