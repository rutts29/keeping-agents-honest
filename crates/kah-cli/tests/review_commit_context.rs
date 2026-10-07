//! git/git v2.53.0 Documentation/git-diff.adoc and git-ls-tree.adoc:
//! endpoint comparison, -z paths and an absent path's empty tree listing.
use kah_cli::observer_review::commit_context;
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn commit(root: &Path) -> String {
    git(root, &["add", "--all"]);
    git(
        root,
        &[
            "-c",
            "user.name=kah fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}
#[test]
fn exact_endpoints_keep_whole_changed_content_and_delete_records() {
    let root = std::env::temp_dir().join(format!(
        "kah-commit-context-{}",
        kah_cli::relay_store::nonce()
    ));
    fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    fs::write(root.join("unchanged"), "untouched").unwrap();
    fs::write(root.join("deleted"), "old").unwrap();
    fs::write(root.join("space name"), "before").unwrap();
    let base = commit(&root);
    let whole = "Whole meaningful sentence.\n".repeat(2000);
    fs::write(root.join("space name"), &whole).unwrap();
    fs::remove_file(root.join("deleted")).unwrap();
    fs::write(root.join("binary"), [0, 255, 10, 128]).unwrap();
    let head = commit(&root);
    fs::write(
        root.join("space name"),
        "uncommitted content is not this SHA",
    )
    .unwrap();
    let packet = commit_context::collect(&root, Some(&base)).unwrap();
    assert_eq!(packet["head"], head);
    assert_eq!(packet["baseline"], base);
    let files = packet["touched_files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert!(!files.iter().any(|file| file["path"] == "unchanged"));
    assert_eq!(
        files
            .iter()
            .find(|file| file["path"] == "space name")
            .unwrap()["content"]["text"],
        whole
    );
    assert_eq!(
        files.iter().find(|file| file["path"] == "deleted").unwrap()["deleted"],
        true
    );
    assert_eq!(
        files.iter().find(|file| file["path"] == "binary").unwrap()["content"]["bytes"],
        serde_json::json!([0, 255, 10, 128])
    );
    let initial = commit_context::collect(&root, None).unwrap();
    assert!(
        initial["touched_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "unchanged")
    );
    assert!(
        commit_context::collect(&root, Some("HEAD; false"))
            .unwrap_err()
            .to_string()
            .contains("REVIEW_COMMIT_BASELINE")
    );
    fs::remove_dir_all(root).unwrap();
}
