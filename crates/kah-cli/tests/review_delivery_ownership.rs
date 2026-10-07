use kah_cli::observer_review::{
    checkpoint,
    state::{Purpose, Request, State, Store},
};
use kah_observer::review_evidence::Document;
use serde_json::json;
use std::path::Path;
fn state(root: &Path) -> State {
    serde_json::from_value(json!({"worker":{"provider":"codex","session":"worker","project":root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
}
fn request() -> Request {
    Request {
        id: "review-delivery".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":2,"goal":"Task","items":[]}),
        trigger: json!({"events":[{"kind":"run_completion","completion_key":{"commit":"a".repeat(40)}}],"verification_followup":{"stage":"checks_complete"}}),
        documents: vec![Document::new(
            "committed-snapshot",
            "committed_code",
            "git:HEAD",
            "code",
        )],
        created_ms: 1,
    }
}
#[test]
fn final_delivery_keeps_ownership_through_routine_claims_but_not_new_facts() {
    let root = std::env::temp_dir().join(format!("kah-delivery-owner-{}", std::process::id()));
    let mut store = Store::delivery_at(&root.join("test-lock"), false).unwrap();
    store.directory = root.join("review");
    kah_core::run_store::ensure_private_directory(&store.directory).unwrap();
    let req = request();
    let final_result = json!({"assessment":{"state":"checkpoint_needs_check","passed":false,"violations":[],"unproven":["normalize"],"needs_thinker":["normalize"],"needs_user":[],"deferrals":[]}});
    let claim_result = json!({"assessment":{"state":"checkpoint_needs_thinker","passed":null,"violations":[],"needs_thinker":["scope"],"needs_user":[],"unproven":["scope"],"deferrals":[]}});
    let mut s = state(&root);
    checkpoint::record(&store, &mut s, &req, &final_result).unwrap();
    let original = s.checkpoint.clone().unwrap();
    let mut claim = req.clone();
    claim.id = "review-claim".into();
    claim.trigger = json!({"events":[{"kind":"native_phase_claim"}]});
    checkpoint::record(&store, &mut s, &claim, &claim_result).unwrap();
    assert_eq!(
        s.checkpoint.as_ref().unwrap()["request"],
        original["request"]
    );
    assert_eq!(s.checkpoint.as_ref().unwrap()["passed"], false);
    assert_eq!(
        s.checkpoint.as_ref().unwrap()["completion_key"],
        original["completion_key"]
    );
    for change in ["authority", "evidence", "violation"] {
        let mut s = state(&root);
        checkpoint::record(&store, &mut s, &req, &final_result).unwrap();
        let mut new = claim.clone();
        let mut result = claim_result.clone();
        match change {
            "authority" => new.intent["revision"] = 3.into(),
            "evidence" => {
                new.documents[0] = Document::new(
                    "committed-snapshot",
                    "committed_code",
                    "git:HEAD",
                    "changed evidence",
                )
            }
            _ => {
                result["assessment"]["passed"] = false.into();
                result["assessment"]["violations"] = json!(["new-violation"]);
            }
        }
        checkpoint::record(&store, &mut s, &new, &result).unwrap();
        assert_eq!(s.checkpoint.as_ref().unwrap()["request"], new.id);
        if change == "violation" {
            assert_eq!(
                s.checkpoint.as_ref().unwrap()["completion_key"],
                original["completion_key"]
            );
        }
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn assistant_only_growth_is_not_evidence_but_new_tool_output_is() {
    use kah_observer::review_evidence::NativeRecord;
    let root = std::env::temp_dir().join(format!("kah-delivery-growth-{}", std::process::id()));
    let mut store = Store::delivery_at(&root.join("test-lock"), false).unwrap();
    store.directory = root.join("review");
    kah_core::run_store::ensure_private_directory(&store.directory).unwrap();
    let mut req = request();
    // NativeRecord is kah's normalized, trusted renderer format. Provider
    // payloads were validated in capture/rendering.rs before constructing it.
    let mut transcript =
        Document::new("transcript", "worker_transcript", "native:worker", "PASS\n");
    transcript.native_records.push(NativeRecord{start_byte:0,end_byte:5,context:json!({"record_type":"commandExecution","record_id":"tool-one","output_ranges":[[0,5]]})});
    req.documents.push(transcript);
    let result = json!({"assessment":{"state":"checkpoint_passed","passed":true,"violations":[],"unproven":[],"needs_user":[],"needs_thinker":[],"deferrals":[]}});
    let mut s = state(&root);
    checkpoint::record(&store, &mut s, &req, &result).unwrap();
    let mut claim = req.clone();
    claim.id = "review-claim-growth".into();
    claim.trigger = json!({"events":[{"kind":"native_phase_claim"}]});
    let d = claim.documents.last_mut().unwrap();
    d.text.push_str("Done.\n");
    d.digest = "changed-transcript-prefix".into();
    d.native_records.push(NativeRecord {
        start_byte: 5,
        end_byte: 11,
        context: json!({"record_type":"agentMessage","record_id":"claim"}),
    });
    checkpoint::record(&store, &mut s, &claim, &result).unwrap();
    assert_eq!(s.checkpoint.as_ref().unwrap()["request"], req.id);
    let d = claim.documents.last_mut().unwrap();
    d.text.push_str("NEW CHECK\n");
    d.native_records.push(NativeRecord{start_byte:11,end_byte:21,context:json!({"record_type":"commandExecution","record_id":"tool-new","output_ranges":[[11,21]]})});
    checkpoint::record(&store, &mut s, &claim, &result).unwrap();
    let invalidated = s.checkpoint.clone().unwrap();
    assert_eq!(invalidated["request"], claim.id);
    assert!(invalidated["passed"].is_null());
    assert!(invalidated["delivery_invalidated"].is_object());
    let mut repeated = claim;
    repeated.id = "review-same-evidence".into();
    checkpoint::record(&store, &mut s, &repeated, &result).unwrap();
    assert_eq!(s.checkpoint.unwrap(), invalidated);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
