//! Exact native checkpoint read-source failure on a virtual relative CAS path.
use kah_cli::observer_review::source_store;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::PathBuf, process::Command};

fn fixture() -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("source-read-{}", kah_cli::relay_store::nonce()));
    let private = root.join("private");
    let workspace = root.join("observer");
    let view = workspace.join("inputs/current");
    fs::create_dir_all(&view).unwrap();
    source_store::initialize(&private, &workspace).unwrap();
    (root, private, workspace)
}
fn publish(private: &std::path::Path, workspace: &std::path::Path) {
    let view = workspace.join("inputs/current");
    fs::write(view.join("context-input.json"), json!({"sources":[{"path":view.join("record.txt")}],"full_request":view.join("request.json")}).to_string()).unwrap();
    source_store::share(private, &view).unwrap();
}
fn cli(workspace: &std::path::Path, digest: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kah"))
        .current_dir(workspace)
        .args(["observe", "read-source", &format!("sources/{digest}")])
        .output()
        .unwrap()
}
/// Every page of a source (read-source serves 16,000 bytes at a time), footers removed.
fn read_all(workspace: &std::path::Path, digest: &str) -> Vec<u8> {
    let (mut all, mut page) = (vec![], 1);
    loop {
        let out = Command::new(env!("CARGO_BIN_EXE_kah"))
            .current_dir(workspace)
            .args([
                "observe",
                "read-source",
                &format!("sources/{digest}"),
                "--page",
                &page.to_string(),
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        match text.rfind("\n[page ").filter(|_| text.ends_with("]\n")) {
            Some(at) => {
                all.extend_from_slice(&text.as_bytes()[..at]);
                page += 1;
            }
            None => {
                all.extend_from_slice(text.as_bytes());
                return all;
            }
        }
    }
}

#[test]
fn native_relative_virtual_document_resolves_and_expands_its_complete_catalogue() {
    let (root, private, workspace) = fixture();
    let view = workspace.join("inputs/current");
    let document = json!({"id":"worker-transcript", "kind":"worker_transcript", "text":"whole source evidence"});
    source_store::text(&private, &view.join("record.txt"), "whole source evidence").unwrap();
    source_store::save(&view, "request.json", &json!({"documents":[document]})).unwrap();
    let stored: Value =
        serde_json::from_slice(&fs::read(view.join("request.json")).unwrap()).unwrap();
    let digest = stored["documents"][0]["$kah_source"].as_str().unwrap();
    publish(&private, &workspace);
    assert!(
        !workspace.join("sources").join(digest).exists(),
        "public source path is intentionally virtual"
    );
    let output = cli(&workspace, digest);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        document
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn virtual_segmented_text_reads_every_byte_and_rejects_corrupt_published_bytes() {
    let (root, private, workspace) = fixture();
    let view = workspace.join("inputs/current");
    let text = "whole record\n".repeat(20000);
    source_store::text(&private, &view.join("record.txt"), &text).unwrap();
    source_store::text(&private, &private.join("unrelated.txt"), "unrelated source").unwrap();
    source_store::save(&view, "request.json", &json!({"head":"unit"})).unwrap();
    let reference: Value =
        serde_json::from_slice(&fs::read(view.join("record.txt")).unwrap()).unwrap();
    let digest = reference["$kah_source"].as_str().unwrap();
    let unrelated: Value =
        serde_json::from_slice(&fs::read(private.join("unrelated.txt")).unwrap()).unwrap();
    publish(&private, &workspace);
    assert!(
        !view
            .join("sources")
            .join(unrelated["$kah_source"].as_str().unwrap())
            .exists()
    );
    let output = cli(&workspace, digest);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(read_all(&workspace, digest), text.as_bytes());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("[page 1 of 17 - continue: bin/kah observe read-source sources/")
    );
    fs::set_permissions(
        view.join("sources").join(digest),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::write(view.join("sources").join(digest), "corrupt published copy").unwrap();
    let bad = cli(&workspace, digest);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("REVIEW_SOURCE_DIGEST"));
    source_store::verify_blob(&private.join("sources").join(digest)).unwrap();
    let missing = cli(&workspace, &"0".repeat(64));
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("REVIEW_SOURCE_IO stage=source_store")
    );
    fs::remove_dir_all(root).unwrap();
}
