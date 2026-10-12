//! Canonical Option1 text reuses structured CAS; original bytes own citations.
use kah_cli::observer_review::source_store;
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn request(head: &str, changed: bool) -> Value {
    let files = (0..20)
        .map(|i| {
            let text = format!("file {i}: whole Unicode 🦀 and quotes \"kept\".\n").repeat(2400);
            let text = if changed && i == 0 {
                format!("changed\n{text}")
            } else {
                text
            };
            (format!("file-{i}"), json!({"revision":head,"text":text}))
        })
        .collect::<serde_json::Map<_, _>>();
    let job = json!({"stage":"confirmation","input":{"revision":head,"files":files}});
    let text = serde_json::to_string(&job).unwrap();
    json!({"trigger":{"option1":job},"documents":[Document::new("option1","literal_first","kah:option1",&text)]})
}

fn stored_bytes(root: &Path) -> u64 {
    fs::read_dir(root.join("sources"))
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum()
}

#[test]
fn consecutive_heads_store_new_content_instead_of_another_opaque_whole_job() {
    let root = std::env::temp_dir().join(format!("kah-structured-job-{}", std::process::id()));
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    let first = request("HEAD_ONE", false);
    source_store::save(&root, "first.json", &first).unwrap();
    publish(&root, "first", &first);
    let before = stored_bytes(&root);
    let second = request("HEAD_TWO", true);
    source_store::save(&root, "second.json", &second).unwrap();
    publish(&root, "second", &second);
    let added = stored_bytes(&root) - before;
    let changed_bytes = second["trigger"]["option1"]["input"]["files"]["file-0"]["text"]
        .as_str()
        .unwrap()
        .len();
    assert!(
        added < (changed_bytes * 3) as u64,
        "new bytes={added}; changed content={changed_bytes}"
    );
    for (name, expected) in [("first.json", &first), ("second.json", &second)] {
        let restored: Value = source_store::read(&root.join(name)).unwrap();
        assert_eq!(restored, *expected);
        assert_eq!(
            restored["documents"][0]["text"],
            expected["documents"][0]["text"]
        );
        assert_eq!(
            restored["documents"][0]["digest"],
            expected["documents"][0]["digest"]
        );
    }
    eprintln!("STRUCTURED_JOB_BYTES before={before} added={added} changed={changed_bytes}");
    fs::remove_dir_all(root).unwrap();
}

fn publish(root: &Path, name: &str, request: &Value) {
    let input = root.join("workspace/inputs").join(name);
    fs::create_dir_all(&input).unwrap();
    let document: Document = serde_json::from_value(request["documents"][0].clone()).unwrap();
    let path = input.join("option1.txt");
    source_store::document(root, &path, &document).unwrap();
    source_store::share_preparation(root, &input, std::slice::from_ref(&path)).unwrap();
    assert_eq!(
        source_store::load_text(&path).unwrap().as_bytes(),
        document.text.as_bytes()
    );
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["encoding"], "json_text");
}

#[test]
fn noncanonical_bytes_stay_verbatim_and_corrupt_jobs_never_render() {
    let root = std::env::temp_dir().join(format!("kah-job-integrity-{}", std::process::id()));
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    let text = " {\"stage\": \"confirmation\"} \n";
    let document = Document::new("option1", "literal_first", "kah:option1", text);
    let alias = root.join("workspace/option1.txt");
    source_store::document(&root, &alias, &document).unwrap();
    assert_eq!(
        source_store::load_text(&alias).unwrap().as_bytes(),
        text.as_bytes()
    );
    let canonical = Document::new(
        "option1",
        "literal_first",
        "kah:option1",
        "{\"stage\":\"confirmation\"}",
    );
    source_store::document(&root, &alias, &canonical).unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&alias).unwrap()).unwrap();
    fs::write(
        root.join("sources")
            .join(saved["$kah_source"].as_str().unwrap()),
        b"tampered",
    )
    .unwrap();
    assert!(
        source_store::load_text(&alias)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_SOURCE_DIGEST")
    );
    fs::remove_dir_all(root).unwrap();
}
