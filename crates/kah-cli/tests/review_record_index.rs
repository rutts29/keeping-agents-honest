//! B': existing kah NativeRecord offsets; serde_json RawValue preserves JSON units.
use kah_cli::observer_review::{indexed_sources, source_store};
use kah_observer::review_evidence::{Document, NativeRecord};
use serde_json::json;
#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn indexes_return_exact_whole_records_and_original_citation_binding() {
    let root = std::env::temp_dir().join(format!(
        "kah-record-index-{}",
        kah_cli::relay_store::nonce()
    ));
    let private = root.join("private");
    let input = root.join("input");
    source_store::initialize(&private, &input).unwrap();
    let text = "First complete message.\nSecond complete message.\n";
    let mut doc = Document::new("transcript", "worker_transcript", "native:worker", text);
    let end = text.find("Second").unwrap();
    doc.native_records = vec![
        NativeRecord {
            start_byte: 0,
            end_byte: end,
            context: json!({"path":"native/0","record_type":"userMessage"}),
        },
        NativeRecord {
            start_byte: end,
            end_byte: text.len(),
            context: json!({"path":"native/1","record_type":"agentMessage"}),
        },
    ];
    let index = indexed_sources::publish_document(&private, &input, &doc).unwrap();
    source_store::share(&private, &input).unwrap();
    assert_eq!(index["records"].as_array().unwrap().len(), 2);
    let first = &index["records"][0];
    let got =
        source_store::load_text(std::path::Path::new(first["path"].as_str().unwrap())).unwrap();
    assert_eq!(got, &text[..end]);
    assert_eq!(first["source_digest"], doc.digest);
    let prior = "{\"report\":{\"decision_units\":{\"requirements\":[{\"id\":\"one\", \"reasoning\":\"whole sentence.\"}]}}}";
    let doc = Document::new("previous-review", "prior_review", "review:old", prior);
    let index = indexed_sources::publish_document(&private, &input, &doc).unwrap();
    source_store::share(&private, &input).unwrap();
    let rec = &index["records"][0];
    let got = source_store::load_text(std::path::Path::new(rec["path"].as_str().unwrap())).unwrap();
    assert_eq!(got, "{\"id\":\"one\", \"reasoning\":\"whole sentence.\"}");
    assert!(prior.contains(&got));
    std::fs::remove_dir_all(root).unwrap();
}
