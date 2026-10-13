//! Real VF16: suggested commands cannot replace a non-executable obligation.
use super::*;
use crate::observer_review::source_store;
use std::fs;
fn retained() -> (Request, Report, Value) {
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-obligation-routing/retained.json"
    ))
    .unwrap();
    (
        serde_json::from_value(value["request"].clone()).unwrap(),
        serde_json::from_value(value["report"].clone()).unwrap(),
        value["assessment"].clone(),
    )
}
#[test]
fn exact_method_conflict_stages_semantic_context_and_keeps_normalize_unconfirmed() {
    let (request, report, mut assessment) = retained();
    for id in ["req-intent", "req-drift", "req-plan", "req-preserve"] {
        assert!(
            semantic_resolution::semantic(&report, id),
            "linked non-executable {id}"
        );
    }
    assert!(!semantic_resolution::semantic(&report, "req-normalize"));
    let root = replay_root();
    assert!(
        semantic_resolution::guard_with(&root, &request, &report, &mut assessment, |_| Ok(()))
            .unwrap()
    );
    assert_eq!(assessment["semantic_resolution"]["status"], "pending");
    assert!(assessment["passed"].is_null());
    assert!(
        assessment["needs_thinker"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "req-normalize")
    );
    assert!(assessment["needs_check"].as_array().unwrap().is_empty());
    assert!(finish_followup(&root, &request, &report, &assessment).unwrap());
    assert!(
        !root.join("gate-followups").exists(),
        "no send-back before resolution"
    );
    let saved: Request = source_store::read(&std::path::PathBuf::from(
        assessment["semantic_resolution"]["record"]
            .as_str()
            .unwrap(),
    ))
    .unwrap();
    assert!(saved.documents.iter().any(|d| d.id == "semantic-state"));
    let old_record = assessment["semantic_resolution"]["record"].clone();
    assert!(
        semantic_resolution::guard_with(&root, &request, &report, &mut assessment, |_| panic!(
            "one snapshot only"
        ))
        .unwrap()
    );
    assert_eq!(assessment["semantic_resolution"]["record"], old_record);
    fs::remove_dir_all(root).unwrap();
}

fn replay_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "semantic-obligation-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_save_directory(&root.join("sources")).unwrap();
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-obligation-routing/retained.json"
    ))
    .unwrap();
    source_store::save(&root, "queue-order.json", &value["queue"]["order"]).unwrap();
    source_store::save(&root, "queue-metadata.json", &value["queue"]["metadata"]).unwrap();
    root
}

#[test]
fn semantic_binary_code_is_a_lossless_descriptor_not_an_inline_byte_array() {
    let root =
        std::env::temp_dir().join(format!("semantic-binary-{}", crate::relay_store::nonce()));
    let (tree, head, blob) = binary_tree(&root);
    let (mut request, _, _) = retained();
    request.documents.clear();
    let private = root.join("review");
    let workspace = root.join("workspace");
    source_store::initialize(&private, &workspace).unwrap();
    semantic_resolution::append_code(&tree, &head, &private, &workspace, &mut request).unwrap();
    let doc = request
        .documents
        .iter()
        .find(|d| d.uri.ends_with(":asset.bin"))
        .unwrap();
    let value: Value = serde_json::from_str(&doc.text).unwrap();
    assert_eq!(value["encoding"], "binary");
    assert_eq!(value["path"], "asset.bin");
    assert_eq!(value["git_blob"], blob);
    assert_eq!(value["size"], 4);
    assert!(value.get("bytes").is_none());
    let digest = value["sha256"].as_str().unwrap();
    assert_eq!(
        fs::read(private.join("sources").join(digest)).unwrap(),
        [255, 0, 42, 254]
    );
    assert_eq!(
        source_store::expand(value["source"].clone(), &private.join("sources")).unwrap(),
        value["source"]
    );
    source_store::verify_blob(Path::new(value["read_path"].as_str().unwrap())).unwrap();
    assert_eq!(
        request.trigger["semantic_resolution"]["binary_sources"]["asset.bin"],
        value["source"]
    );
    fs::remove_dir_all(root).unwrap();
}
fn binary_tree(root: &Path) -> (kah_run::worktree::Worktree, String, String) {
    let source = root.join("source");
    kah_core::run_store::ensure_save_directory(&source).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-C",
                source.to_str().unwrap(),
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    fs::write(source.join("asset.bin"), [255, 0, 42, 254]).unwrap();
    git(&["add", "asset.bin"]);
    git(&["commit", "-qm", "binary fixture"]);
    let head = git(&["rev-parse", "HEAD"]);
    let blob = git(&["rev-parse", "HEAD:asset.bin"]);
    let bundle = root.join("source.bundle");
    git(&["bundle", "create", bundle.to_str().unwrap(), "HEAD"]);
    let tree = kah_run::worktree::Worktree::from_bundle(
        &source,
        &root.join("candidate"),
        &bundle,
        &head,
        "run-binary-fixture",
    )
    .unwrap();
    (tree, head, blob)
}
