use kah_cli::observer_review::{
    commit_packet,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::{Citation, Document};
use serde_json::json;
#[test]
fn diff_packet_preserves_the_complete_tree_and_expands_a_whole_outside_file() {
    let whole = "Whole untouched sentence.\n".repeat(2000);
    let (mut request, project) = fixture(&whole);
    let file = Document::new(
        &commit_packet::file_id("untouched.rs"),
        "committed_code",
        "git:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee:untouched.rs",
        &whole,
    );
    assert!(!request.documents.iter().any(|d| d.id == file.id));
    let citation = Citation {
        source: file.id.clone(),
        digest: file.digest.clone(),
        quote: "Whole untouched sentence.\n".into(),
        occurrence: 0,
    };
    assert!(
        commit_packet::validate_report(&request, &json!({"citations":[&citation]}))
            .unwrap_err()
            .to_string()
            .contains("REVIEW_SOURCE_OUTSIDE_PACKET")
    );
    let report = json!({"findings":[{"citations":[citation]}]});
    assert_eq!(
        commit_packet::expand(&mut request, &report).unwrap(),
        vec![file.id.clone()]
    );
    assert_eq!(
        request
            .documents
            .iter()
            .find(|d| d.id == file.id)
            .unwrap()
            .text,
        whole
    );
    assert_scope(&request, &project);
    assert!(
        commit_packet::expand(&mut request, &report)
            .unwrap()
            .is_empty()
    );
    assert!(
        commit_packet::expand(
            &mut request,
            &json!({"citations":[{"source":"invented","digest":"bad","quote":"fake"}]})
        )
        .unwrap()
        .is_empty()
    );
}

fn fixture(whole: &str) -> (Request, serde_json::Value) {
    let mut project = json!({"head":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","committed_snapshot":"full lossless tree","commit_context":{"baseline":"old","head":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","diff":"entire diff","touched_files":[{"path":"changed.rs","content":{"text":"changed entire file"},"blob":"a"}]},"full_tree_files":[{"path":"changed.rs","content":{"text":"changed entire file"},"blob":"a"},{"path":"untouched.rs","content":{"text":whole},"blob":"b"}]});
    let docs = commit_packet::documents(&mut project).unwrap();
    let request = Request {
        id: "review-packet".into(),
        worker: "fixture".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into()),
        intent: json!({"goal":"whole authority"}),
        trigger: json!({"review_class":"commit"}),
        documents: docs,
        created_ms: 0,
    };
    (request, project)
}

fn assert_scope(request: &Request, project: &serde_json::Value) {
    assert_eq!(project["review_diff"]["baseline"], "old");
    assert_eq!(
        project["review_diff"]["head"],
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    );
    assert_eq!(
        request
            .documents
            .iter()
            .find(|d| d.id == "diff")
            .unwrap()
            .text,
        "entire diff"
    );
    assert!(!project.to_string().contains("full lossless tree"));
    assert!(
        request
            .documents
            .iter()
            .any(|d| d.kind == "lossless_tree_catalog")
    );
}
#[test]
fn host_expansion_requires_its_own_request_and_unchanged_report_digest() {
    use kah_cli::observer_review::{
        source_store,
        state::{State, Store},
    };
    use sha2::{Digest, Sha256};
    use std::fs;
    let (mut request, _) = fixture("whole outside file");
    request.worker = format!("packet-recovery-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&request.worker).unwrap();
    let root = store.directory.join("fixture-workspace");
    fs::create_dir(&root).unwrap();
    let state: State = serde_json::from_value(json!({"worker":{"provider":"codex","session":request.worker,"project":root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let report = root.join("rejected.json");
    let bytes =
        serde_json::to_vec(&json!({"source":commit_packet::file_id("untouched.rs")})).unwrap();
    fs::write(&report, &bytes).unwrap();
    source_store::save(&store.directory, "validation-current.json", &json!({"request":request.id,"request_digest":format!("{:x}",Sha256::digest(serde_json::to_vec(&request).unwrap())),"report":report,"report_digest":format!("{:x}",Sha256::digest(&bytes))})).unwrap();
    let expanded = commit_packet::expand_rejected(&store, &state, &request).unwrap();
    assert_eq!(expanded.documents.len(), request.documents.len() + 1);
    assert_eq!(expanded.id, request.id);
    assert_eq!(expanded.intent, request.intent);
    assert_eq!(
        fs::read_dir(store.directory.join("scope-expansions"))
            .unwrap()
            .count(),
        1
    );
    fs::write(&report, "changed after validation").unwrap();
    assert_eq!(
        commit_packet::expand_rejected(&store, &state, &request)
            .unwrap()
            .documents
            .len(),
        request.documents.len()
    );
    fs::remove_file(&report).unwrap();
    let outside = store.directory.join("outside-report.json");
    fs::write(&outside, &bytes).unwrap();
    std::os::unix::fs::symlink(&outside, &report).unwrap();
    assert!(commit_packet::expand_rejected(&store, &state, &request).is_err());
    let state_root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(state_root).unwrap();
    assert!(!root.exists());
}
#[test]
fn persisted_catalog_keeps_only_an_index_in_the_packet_and_recovers_complete_sources() {
    use kah_cli::observer_review::{
        source_store,
        state::{State, Store},
    };
    use std::fs;
    let whole = "Whole unchanged authority and code.\n".repeat(2000);
    let (mut request, _) = fixture(&whole);
    request.worker = format!("packet-storage-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&request.worker).unwrap();
    let root = store.directory.join("fixture-workspace");
    fs::create_dir(&root).unwrap();
    source_store::initialize(&store.directory, &root).unwrap();
    let state: State = serde_json::from_value(json!({"worker":{"provider":"codex","session":request.worker,"project":root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    commit_packet::persist(&state, &mut request.documents).unwrap();
    let catalog = request
        .documents
        .iter()
        .find(|d| d.id == "whole-tree-catalog")
        .unwrap();
    assert!(!catalog.text.contains("Whole unchanged"));
    let index: serde_json::Value = serde_json::from_str(&catalog.text).unwrap();
    let stored: serde_json::Value =
        source_store::read(std::path::Path::new(index["full_source"].as_str().unwrap())).unwrap();
    assert_eq!(stored["state"]["files"][1]["text"], whole);
    let added = commit_packet::expand(
        &mut request,
        &json!({"source":commit_packet::file_id("untouched.rs")}),
    )
    .unwrap();
    assert_eq!(added, vec![commit_packet::file_id("untouched.rs")]);
    assert_eq!(request.documents.last().unwrap().text, whole);
    let state_root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(state_root).unwrap();
    assert!(!root.exists());
}
#[test]
fn steering_scope_does_not_advance_the_last_code_reviewed_sha() {
    use kah_cli::observer_review::{source_store, state::Store};
    let (mut request, _) = fixture("whole file");
    request.worker = format!("scope-baseline-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&request.worker).unwrap();
    source_store::save(
        &store.directory,
        "last-reviewed-sha.json",
        &json!({"head":"old-reviewed-sha"}),
    )
    .unwrap();
    request.purpose = Purpose::Steering;
    request.trigger = commit_packet::trigger(&request.purpose, json!({}));
    assert!(commit_packet::is_scoped(&request));
    assert!(!commit_packet::is_commit(&request));
    commit_packet::reviewed(&store, &request).unwrap();
    let baseline: serde_json::Value =
        source_store::read(&store.directory.join("last-reviewed-sha.json")).unwrap();
    assert_eq!(baseline["head"], "old-reviewed-sha");
    request.purpose = Purpose::Drift;
    commit_packet::reviewed(&store, &request).unwrap();
    let baseline: serde_json::Value =
        source_store::read(&store.directory.join("last-reviewed-sha.json")).unwrap();
    assert_eq!(baseline["head"], "old-reviewed-sha");
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
