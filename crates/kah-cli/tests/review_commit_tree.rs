//! git/git v2.53.0 Documentation/git-cat-file.adoc, BATCH OUTPUT:
//! each blob is framed by its declared size, not by lines in its content.
use kah_cli::observer_review::commit_context;
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
#[test]
fn complete_tree_keeps_untouched_binary_and_symlink_bytes_without_following_links() {
    let root =
        std::env::temp_dir().join(format!("kah-commit-tree-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    fs::write(root.join("untouched"), "whole untouched file\n").unwrap();
    fs::write(root.join("binary"), [0, 255, 10, 0, 128]).unwrap();
    std::os::unix::fs::symlink("/host-secret-must-not-be-followed", root.join("link")).unwrap();
    git(&root, &["add", "--all"]);
    git(
        &root,
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
    let head = git(&root, &["rev-parse", "HEAD"]);
    let files = commit_context::tree(&root, &head).unwrap();
    assert_eq!(files.len(), 3);
    let get = |path: &str| files.iter().find(|f| f["path"] == path).unwrap();
    assert_eq!(
        get("untouched")["content"]["text"],
        "whole untouched file\n"
    );
    assert_eq!(
        get("binary")["content"]["bytes"],
        serde_json::json!([0, 255, 10, 0, 128])
    );
    assert_eq!(
        get("link")["content"]["text"],
        "/host-secret-must-not-be-followed"
    );
    assert_eq!(get("link")["mode"], "120000");
    fs::remove_dir_all(root).unwrap();
}
