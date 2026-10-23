use super::super::super::state::Purpose;
use super::*;
use serde_json::json;

#[test]
fn host_reproduces_exact_current_report_rejection_and_rejects_stale_or_forged_records() {
    let root = std::env::temp_dir().join(format!("kah-rejected-report-{}", std::process::id()));
    let outbox = root.join("outbox");
    fs::create_dir_all(&outbox).unwrap();
    let report = outbox.join("review-test.json");
    let bytes = b"{\"request_id\":\"review-test\",\"unknown\":true}";
    fs::write(&report, bytes).unwrap();
    let request = Request {
        id: "review-test".into(),
        worker: "disposable-worker".into(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let error = workspace::validate_report_bytes(&request, bytes)
        .err()
        .unwrap();
    let actual = submit(&root, &request, &report).unwrap_err();
    assert_eq!(actual.to_string(), error.to_string());
    assert!(
        verified(&outbox, &request)
            .unwrap()
            .unwrap()
            .contains("REVIEW_REPORT_JSON")
    );
    let mut other = request.clone();
    other.turn = "other".into();
    assert!(verified(&outbox, &other).unwrap().is_none());
    fs::write(&report, b"different report").unwrap();
    assert!(verified(&outbox, &request).unwrap().is_none());
    fs::write(&report, bytes).unwrap();
    let invented = crate::observer_review::failure::validation::<()>(
        crate::observer_review::failure::Origin::ReportValidation,
        || Err("invented validation failure".into()),
    )
    .unwrap_err();
    record(&outbox, &request, &report, bytes, invented.as_ref()).unwrap();
    assert!(verified(&outbox, &request).unwrap().is_none());
    record(&outbox, &request, &report, bytes, error.as_ref()).unwrap();
    fs::write(outbox.join("submitted-review-test.json"), "{}").unwrap();
    assert!(
        verified(&outbox, &request).unwrap().is_some(),
        "forged ACK is not evidence"
    );
    fs::remove_dir_all(root).unwrap();
}

fn submit(root: &Path, request: &Request, report: &Path) -> Result<()> {
    let input = root.join("inputs/review-test");
    fs::create_dir_all(&input)?;
    let request_path = input.join("request.json");
    fs::write(&request_path, serde_json::to_vec(request)?)?;
    workspace::submit(&[
        "observe".into(),
        "report".into(),
        "--request".into(),
        request_path.to_str().unwrap().into(),
        "--json".into(),
        report.to_str().unwrap().into(),
    ])
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn a_second_request_with_the_same_anchor_refreshes_a_changed_report_before_expansion() {
    use crate::observer_review::{
        commit_packet, packet_references, source_store,
        state::{State, Store},
    };
    let worker = format!("changed-anchor-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = &store.directory;
    let workspace = root.join("observer");
    let outbox = workspace.join("outbox");
    fs::create_dir_all(&outbox).unwrap();
    source_store::initialize(root, &workspace).unwrap();
    let state:State = serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let original = kah_observer::review_evidence::Document::new(
        "historical-anchor",
        "historical_deferral_anchor",
        "native:worker",
        "Whole historical requirement.",
    );
    fs::create_dir_all(root.join("requests")).unwrap();
    for id in ["review-first", "review-second"] {
        let mut request = Request {
            id: id.into(),
            worker: worker.clone(),
            purpose: Purpose::Drift,
            turn: "t".into(),
            head: None,
            intent: json!({}),
            trigger: json!({"packet_scope":"diff-v1"}),
            documents: vec![original.clone()],
            created_ms: 0,
        };
        packet_references::scope(root, &workspace, &mut request).unwrap();
        source_store::save(
            &root.join("requests"),
            &format!("{id}.json"),
            &serde_json::to_value(&request).unwrap(),
        )
        .unwrap();
        let file = outbox.join(format!("{id}.json"));
        let mut report = json!({"request_id":id,"findings":[{"topic":"drift","claim":"Original finding","citations":[{"source":original.id,"digest":original.digest,"quote":original.text,"occurrence":0}]}]});
        let bytes = serde_json::to_vec(&report).unwrap();
        fs::write(&file, &bytes).unwrap();
        let error = workspace::validate_report_bytes(&request, &bytes).unwrap_err();
        record(&outbox, &request, &file, &bytes, error.as_ref()).unwrap();
        report["findings"][0]["claim"] = "Corrected finding".into();
        fs::write(&file, serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(current(root, &outbox, &worker, id).unwrap().is_some());
        let restored = commit_packet::expand_rejected(&store, &state, &request).unwrap();
        assert_eq!(restored.id, id);
        assert_eq!(restored.documents[0].text, original.text);
        assert_eq!(restored.documents[0].digest, original.digest);
    }
    let directory = root.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(directory).unwrap();
}
