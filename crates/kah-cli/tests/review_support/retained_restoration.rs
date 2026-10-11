//! Offline replay of the actual native commit-scope restoration failure.
use kah_cli::observer_review::{
    commit_packet, redelivery, source_store,
    state::{Purpose, Request, State, Store},
};
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn run(input: &Path) {
    let root =
        std::path::PathBuf::from(std::env::var_os("KAH_RETAINED_REFERENCE_EVIDENCE").unwrap());
    let state_root = kah_core::run_store::RunStore::default_state_root().unwrap();
    assert!(
        state_root.starts_with(&root),
        "RETAINED_FIXTURE_SCOPE stage=prepare"
    );
    let bytes = fs::read(input).unwrap();
    let case: Value = serde_json::from_slice(&bytes).unwrap();
    let original: Request = serde_json::from_value(case["request"].clone()).unwrap();
    assert_eq!(
        original.id,
        case["expected_request_id"]
            .as_str()
            .expect("fixture expected request ID")
    );
    assert_eq!(original.purpose, Purpose::Checkpoint);
    assert_eq!(
        digest(&serde_json::to_vec(&original).unwrap()),
        case["rejection"]["request_digest"]
    );
    let results = [Purpose::Checkpoint, Purpose::Steering]
        .into_iter()
        .map(|purpose| replay(&root, &case, original.clone(), purpose))
        .collect::<Vec<_>>();
    save(
        &root.join("result.json"),
        &json!({"passed":true,"offline_replay":true,
        "input":input,"input_sha256":digest(&bytes),"jev_calls":0,"native_calls":0,
        "original_request":original.id,"original_purpose":original.purpose,
        "cases":results,"final_acceptance":false}),
    );
}

fn replay(root: &Path, case: &Value, mut request: Request, purpose: Purpose) -> Value {
    let label = if purpose == Purpose::Steering {
        "steering"
    } else {
        "checkpoint"
    };
    let worker = format!(
        "retained-restoration-{label}-{}",
        kah_cli::relay_store::nonce()
    );
    let home = root.join(label);
    let state = super::setup(&worker, &home);
    let store = Store::open(&worker).unwrap();
    source_store::initialize(&store.directory, &state.observer.workspace).unwrap();
    request.worker = worker;
    request.purpose = purpose;
    relocate(&state, &mut request, case);
    let report = case["report_raw"].as_str().unwrap().as_bytes();
    assert_eq!(digest(report), case["rejection"]["report_digest"]);
    assert!(
        commit_packet::validate_report(&request, &serde_json::from_slice(report).unwrap())
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_SOURCE_OUTSIDE_PACKET")
    );
    reject(&home, &state, &store, &request, report, case);
    let before = request
        .documents
        .iter()
        .find(|d| d.id == "transcript")
        .unwrap()
        .clone();
    let published = publish(&home, &store, &state, &request);
    assert_eq!(published.id, request.id);
    assert_eq!(published.documents.len(), 17);
    assert_eq!(request.documents.len(), published.documents.len());
    assert!(redelivery::expansion_changed(&request, &published));
    let restored = published
        .documents
        .iter()
        .find(|d| d.id == "transcript")
        .unwrap();
    let expected: Document =
        serde_json::from_value(case["originals"][0]["original"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(
        restored.text.len() as u64,
        case["expected_transcript_bytes"]
            .as_u64()
            .expect("fixture expected transcript size")
    );
    json!({"purpose":request.purpose,"worker":request.worker,"request":request.id,
        "documents_before":request.documents.len(),"documents_after":published.documents.len(),
        "source":"transcript","before_kind":before.kind,"after_kind":restored.kind,
        "before_digest":before.digest,"after_digest":restored.digest,"restored_bytes":restored.text.len(),
        "complete_original_and_native_records_equal":true,"publication_and_repeat_passed":true,
        "repeated_block_notice":false,"private_store":store.directory,"workspace":state.observer.workspace})
}

fn publish(home: &Path, store: &Store, state: &State, request: &Request) -> Request {
    redelivery::prompt(home, store, state, request, &request.id).unwrap();
    let published: Request = source_store::read(
        &state
            .observer
            .workspace
            .join("inputs")
            .join(&request.id)
            .join("request.json"),
    )
    .unwrap();
    // The old rejection belongs to the pre-expansion request, never this publication.
    redelivery::prompt(home, store, state, &published, &request.id).unwrap();
    assert!(!store.directory.join("redelivery-notice.json").exists());
    published
}

fn relocate(state: &State, request: &mut Request, case: &Value) {
    let archive = state.observer.workspace.join("packet-originals");
    fs::create_dir_all(&archive).unwrap();
    for row in case["originals"].as_array().unwrap() {
        let reference: Document = serde_json::from_value(row["reference"].clone()).unwrap();
        let mut descriptor: Value = serde_json::from_str(&reference.text).unwrap();
        assert_eq!(
            digest(&serde_json::to_vec(&row["original"]).unwrap()),
            descriptor["source_digest"]
        );
        let original: Document = serde_json::from_value(row["original"].clone()).unwrap();
        assert_eq!(original.digest, descriptor["original_digest"]);
        let basename = Path::new(descriptor["full_source"].as_str().unwrap())
            .file_name()
            .unwrap();
        let target = archive.join(basename);
        save(&target, &json!({"state":row["original"]}));
        descriptor["full_source"] = json!(target);
        let document = request
            .documents
            .iter_mut()
            .find(|d| d.id == reference.id)
            .unwrap();
        *document = Document::new(
            &reference.id,
            &reference.kind,
            &reference.uri,
            &descriptor.to_string(),
        );
    }
    let catalog = request
        .documents
        .iter_mut()
        .find(|d| d.id == "whole-tree-catalog")
        .unwrap();
    let mut descriptor: Value = serde_json::from_str(&catalog.text).unwrap();
    assert_eq!(
        digest(&serde_json::to_vec(&case["catalog"]).unwrap()),
        descriptor["source_digest"]
    );
    let target = state.observer.workspace.join("catalog.json");
    save(&target, &json!({"state":case["catalog"]}));
    descriptor["full_source"] = json!(target);
    *catalog = Document::new(
        &catalog.id,
        &catalog.kind,
        &catalog.uri,
        &descriptor.to_string(),
    );
}

fn reject(
    home: &Path,
    state: &State,
    store: &Store,
    request: &Request,
    report: &[u8],
    case: &Value,
) {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_kah"), bin.join("kah")).unwrap();
    let outbox = state.observer.workspace.join("outbox");
    fs::create_dir_all(&outbox).unwrap();
    let path = outbox.join(format!("{}.json", request.id));
    fs::write(&path, report).unwrap();
    save(
        &store.directory.join("validation-current.json"),
        &json!({
        "request":request.id,"request_digest":digest(&serde_json::to_vec(request).unwrap()),
        "report":path,"report_digest":digest(report),"error":case["rejection"]["error"]}),
    );
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn save(path: &Path, value: &Value) {
    kah_cli::unattended_report::save(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        value,
    )
    .unwrap();
}
