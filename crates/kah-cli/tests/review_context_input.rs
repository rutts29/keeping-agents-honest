use kah_cli::observer_review::{
    context_input,
    state::{Purpose, Request},
};
use serde_json::json;
use std::path::Path;
fn request(text: &str) -> Request {
    serde_json::from_value(json!({"id":"review-one","worker":"worker","purpose":"preparation","turn":"turn","head":"head","intent":{},"trigger":{},"created_ms":1,"documents":[{"id":"transcript","kind":"transcript","uri":"native:session","text":text,"digest":format!("sha256:{}","a".repeat(64))}]})).unwrap()
}
#[test]
fn context_packet_carries_all_sources_by_reference_not_the_growing_history() {
    let one = request(&"complete history\n".repeat(100000));
    let mut two = one.clone();
    two.documents[0]
        .text
        .push_str("a new complete native record\n");
    let first = context_input::view(&one, Path::new("inputs/review-one"));
    let second = context_input::view(&two, Path::new("inputs/review-one"));
    assert_eq!(
        first["sources"][0]["path"],
        "inputs/review-one/transcript.txt"
    );
    assert_eq!(
        first["sources"].as_array().unwrap().len(),
        one.documents.len()
    );
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("complete history")
    );
    let mut original = first.clone();
    original["sources"][0]["bytes"] = second["sources"][0]["bytes"].clone();
    assert_eq!(
        original, second,
        "only source size metadata changes; old bytes are never recopied"
    );
    assert_eq!(one.purpose, Purpose::Preparation);
}

#[test]
#[ignore = "$0 real retained request/catalogue size measurement; explicit private output required"]
fn measure_retained_context_catalogues() {
    use std::fs;
    let output = std::path::PathBuf::from(std::env::var_os("KAH_CONTEXT_SIZE_OUTPUT").unwrap());
    fs::create_dir(&output).unwrap();
    let mut records = vec![];
    for id in ["review-1000000000009-1", "review-1000000000010-1"] {
        let input =
            std::path::PathBuf::from(std::env::var_os("KAH_CONTEXT_SIZE_INPUT_ROOT").unwrap())
                .join(id);
        let request: Request =
            kah_cli::observer_review::source_store::read(&input.join("request.json")).unwrap();
        let view = context_input::view(&request, &input);
        let bytes = serde_json::to_vec_pretty(&view).unwrap();
        fs::write(output.join(format!("{id}-context-input.json")), &bytes).unwrap();
        records.push(json!({"request":id,"catalogue_bytes":bytes.len(),"source_count":request.documents.len(),"full_source_bytes":request.documents.iter().map(|d|d.text.len()).sum::<usize>(),"provider_calls":0}));
    }
    fs::write(
        output.join("sizes.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
}

#[test]
fn old_pending_request_gets_catalogue_without_expanding_history() {
    use kah_cli::observer_review::source_store;
    let root = std::env::temp_dir().join(format!("context-old-{}", kah_cli::relay_store::nonce()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("inputs");
    std::fs::create_dir_all(&input).unwrap();
    source_store::initialize(&root, &input).unwrap();
    let request = request(&"whole history\n".repeat(100000));
    source_store::save(
        &input,
        "request.json",
        &serde_json::to_value(&request).unwrap(),
    )
    .unwrap();
    source_store::save(
        &input,
        "manifest.json",
        &json!({"documents":[{"id":"transcript","bytes":request.documents[0].text.len()}]}),
    )
    .unwrap();
    context_input::ensure(&input).unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(input.join("context-input.json")).unwrap()).unwrap();
    assert_eq!(
        value["sources"][0]["bytes"],
        request.documents[0].text.len()
    );
    assert_eq!(value["sources"][0]["digest"], request.documents[0].digest);
    assert!(!value.to_string().contains("whole history"));
    std::fs::remove_dir_all(root).unwrap();
}
