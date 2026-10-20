use super::*;

#[test]
fn native_activity_clears_delivery_noise_but_preserves_report_validation() {
    let worker = format!("retry-validation-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let path = store.directory.join("gap.json");
    for (error, retained) in [
        ("REVIEW_PHASE_CLAIM_UNCERTAIN stage=checkpoint", true),
        ("REVIEW_PROTOCOL_FACT_CONTRADICTION stage=report", true),
        ("REVIEW_REPORT_MISSING stage=observer", true),
        ("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery", false),
        ("REVIEW_NOT_IDLE stage=delivery", false),
        ("REVIEW_OBSERVER_STALLED stage=liveness", false),
    ] {
        crate::unattended_report::save(
            &store.directory,
            "gap.json",
            &serde_json::json!({"error":error}),
        )
        .unwrap();
        clear_confirmed_delivery_gap(&path).unwrap();
        assert_eq!(path.exists(), retained, "{error}");
    }
    std::fs::remove_dir_all(store.directory.parent().unwrap()).unwrap();
}

#[test]
fn native_outside_file_rejection_refreshes_stale_host_record_and_expands_same_request() {
    use review::{commit_packet, source_store};
    let worker = format!("outside-handoff-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("observer");
    let input = workspace.join("inputs/request");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir(workspace.join("outbox")).unwrap();
    let state = fake_state(&worker, &workspace);
    store.save(&state).unwrap();
    let request = outside_request(&worker);
    std::fs::create_dir_all(store.directory.join("requests")).unwrap();
    source_store::save(
        &store.directory.join("requests"),
        "review-outside.json",
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    let request_path = input.join("request.json");
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let (outside, error) = reject_outside(&workspace, &request_path, &request);
    assert!(
        error
            .to_string()
            .starts_with("REVIEW_SOURCE_OUTSIDE_PACKET"),
        "{error}"
    );
    crate::unattended_report::save(
        &store.directory,
        "validation-current.json",
        &json!({"request":"retired-stale"}),
    )
    .unwrap();
    refresh_rejection(&store, &state, &request.id).unwrap();
    let gap: Value = review::state::read(&store.directory.join("gap.json"))
        .unwrap()
        .unwrap();
    assert_eq!(gap["request"], request.id);
    assert_eq!(gap["origin"], "report_validation");
    assert!(!diagnostic(&gap).contains("Correct that report error"));
    let expanded = commit_packet::expand_rejected(&store, &state, &request).unwrap();
    assert_eq!(expanded.id, request.id);
    assert_eq!(expanded.documents.last().unwrap().text, outside.text);
    assert!(store.directory.join("scope-expansions").is_dir());
    let parent = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(parent).unwrap();
}
fn fake_state(worker: &str, workspace: &Path) -> State {
    serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
}

fn outside_request(worker: &str) -> review::state::Request {
    use review::{
        commit_packet,
        state::{Purpose, Request},
    };
    let mut project = json!({"head":"a".repeat(40),"committed_snapshot":"whole",
        "commit_context":{"head":"a".repeat(40),"baseline":"empty","diff":"whole diff","touched_files":[]},
        "full_tree_files":[{"path":"outside.rs","content":{"text":"A whole outside sentence.\n"}}]});
    Request {
        id: "review-outside".into(),
        worker: worker.into(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({}),
        trigger: json!({"packet_scope":"diff-v1"}),
        documents: commit_packet::documents(&mut project).unwrap(),
        created_ms: 0,
    }
}

fn reject_outside(
    workspace: &Path,
    request_path: &Path,
    request: &review::state::Request,
) -> (
    kah_observer::review_evidence::Document,
    Box<dyn std::error::Error>,
) {
    use kah_observer::review_evidence::Citation;
    use review::commit_packet;
    let outside = kah_observer::review_evidence::Document::new(
        &commit_packet::file_id("outside.rs"),
        "committed_code",
        &format!("git:{}:outside.rs", "a".repeat(40)),
        "A whole outside sentence.\n",
    );
    let citation = Citation {
        source: outside.id.clone(),
        digest: outside.digest.clone(),
        quote: outside.text.clone(),
        occurrence: 0,
    };
    let report = workspace.join("outbox/report.json");
    std::fs::write(
        &report,
        serde_json::to_vec(&json!({"request_id":request.id,"findings":[{"topic":"drift","claim":"Preserve the outside source","citations":[citation]}]}))
            .unwrap(),
    )
    .unwrap();
    let error = workspace::submit(&[
        "observe".into(),
        "report".into(),
        "--request".into(),
        request_path.to_str().unwrap().into(),
        "--json".into(),
        report.to_str().unwrap().into(),
    ])
    .unwrap_err();
    (outside, error)
}

#[test]
fn a_replaced_observer_gets_the_identical_request_body() {
    assert!(!correction_needed(&serde_json::json!({
        "error":"REVIEW_OBSERVER_EXEC_LOST stage=delivery: exec-server transport disconnected"
    })));
}
