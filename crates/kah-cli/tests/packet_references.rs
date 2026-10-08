use kah_cli::observer_review::{
    packet_references, source_store,
    state::{Purpose, Request, Store},
};
use kah_observer::review_evidence::{Document, NativeRecord};
use serde_json::json;
use std::fs;
fn request(worker: &str) -> Request {
    let mut transcript = Document::new(
        "transcript",
        "worker_transcript",
        "native:codex:worker",
        "Whole native assistant message.\nWhole tool result.\n",
    );
    transcript.native_records = vec![NativeRecord {
        start_byte: 0,
        end_byte: 31,
        context: json!({"record_type":"agentMessage","native_transport_only":true,"record_id":"real-record"}),
    }];
    Request {
        id: "review-reference".into(),
        worker: worker.into(),
        purpose: Purpose::Checkpoint,
        turn: "native-turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Whole original goal.","items":[{"id":"user-original","text":"Never discard my words."}]}),
        trigger: json!({"review_class":"commit","packet_scope":"diff-v1"}),
        documents: vec![
            transcript,
            Document::new(
                "previous-review",
                "prior_review",
                "kah:previous",
                "Whole earlier finding.",
            ),
            Document::new(
                "historical-original",
                "historical_deferral_anchor",
                "kah:old-ledger",
                "Whole historical user authority.",
            ),
            Document::new("diff", "diff", "git:diff", "Whole touched diff."),
        ],
        created_ms: 1,
    }
}
#[test]
fn light_packets_reference_full_sources_and_expand_the_exact_cited_original() {
    let worker = format!("packet-reference-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("workspace");
    source_store::initialize(&store.directory, &workspace).unwrap();
    let mut packet = request(&worker);
    let original = packet.clone();
    packet_references::scope(&store.directory, &workspace, &mut packet).unwrap();
    assert_eq!(packet.intent, original.intent);
    assert_eq!(packet.documents[3].text, original.documents[3].text);
    assert_eq!(packet.documents[0].kind, "lossless_reference");
    assert!(
        !packet.documents[0]
            .text
            .contains("Whole native assistant message.")
    );
    let report = json!({"source":"transcript","digest":original.documents[0].digest,"quote":"Whole native assistant message."});
    assert!(
        kah_cli::observer_review::commit_packet::validate_report(&packet, &report)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_SOURCE_OUTSIDE_PACKET")
    );
    let added = kah_cli::observer_review::commit_packet::expand(&mut packet, &report).unwrap();
    assert_eq!(added, vec!["transcript"]);
    assert_eq!(
        serde_json::to_value(&packet.documents[0]).unwrap(),
        serde_json::to_value(&original.documents[0]).unwrap()
    );
    assert_eq!(packet.id, original.id);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn heavy_packets_stay_whole_and_reference_corruption_fails_closed() {
    let worker = format!("reference-integrity-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("workspace");
    source_store::initialize(&store.directory, &workspace).unwrap();
    let mut heavy = request(&worker);
    heavy.trigger = json!({"review_class":"gate"});
    let before = serde_json::to_value(&heavy).unwrap();
    packet_references::scope(&store.directory, &workspace, &mut heavy).unwrap();
    assert_eq!(serde_json::to_value(&heavy).unwrap(), before);
    let mut packet = request(&worker);
    packet_references::scope(&store.directory, &workspace, &mut packet).unwrap();
    let descriptor: serde_json::Value = serde_json::from_str(&packet.documents[0].text).unwrap();
    fs::write(
        descriptor["full_source"].as_str().unwrap(),
        b"corrupt original descriptor",
    )
    .unwrap();
    assert!(
        kah_cli::observer_review::commit_packet::expand(
            &mut packet,
            &json!({"source":"transcript"})
        )
        .is_err()
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
