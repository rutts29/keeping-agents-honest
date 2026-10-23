use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

/// A native cumulative checkpoint report with one row.
const REPORT: &str = r#"{"request":"review-1700000000000-1","result":{"stage":"checkpoint","rows":[{"requirement":"goal","finding":"The parser and its tests match the plan; the release tag is not visible in the diff.","status":"unclear","kind":"behavior","quotes":[]}]}}"#;
const CAPTURED_HEAD: &str = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";
const NEW_HEAD: &str = "9f8e7d6c5b4a39281706f5e4d3c2b1a098765432";

#[test]
fn a_native_checkpoint_report_creates_its_typed_snapshot_directory() {
    let raw = REPORT;
    let report: super::super::protocol::Report = serde_json::from_str(raw).unwrap();
    let super::super::protocol::Answer::Checkpoint { rows } = report.result else {
        panic!("the fixture must be a native cumulative checkpoint report");
    };
    let cumulative = Request {
        id: report.request,
        worker: "worker-gate".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn-gate".into(),
        head: Some(CAPTURED_HEAD.into()),
        intent: json!({"revision":2}),
        trigger: json!({"option1":{"original_trigger":{"kind":"user_gate","source":"host_cli"}}}),
        documents: vec![Document::new(
            "native-report",
            "retained_native_checkpoint",
            "kah:review-1700000000000-1",
            raw,
        )],
        created_ms: 1_700_000_000_045,
    };
    let mut captured = cumulative.clone();
    captured.id.push_str("-typed");
    let next = prepare(&cumulative, captured, &rows).unwrap();
    let base =
        std::env::temp_dir().join(format!("kah-gate-storage-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&base.join("sources")).unwrap();
    let directory = base.join("explicit-gate-inputs");
    assert!(!directory.exists(), "the snapshot directory starts missing");
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
fn captured_scope_survives_a_newer_live_head_without_mixing_code_intent_or_docs() {
    // The head moved after the cumulative request captured its scope.
    let value = json!({"worker":"worker-scope","captured_head":CAPTURED_HEAD,"new_head":NEW_HEAD,
        "documents":[
            {"path":"PLAN.md","before":"Phase 2: parser and tests.\n","after":"Phase 2: parser, tests and docs.\n"},
            {"path":"src/parse.rs","before":"pub fn parse() {}\n","after":"pub fn parse() -> bool { true }\n"}]});
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
        id: "review-1700000000100-2".into(),
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
        created_ms: 1_700_000_000_198,
    }
}
