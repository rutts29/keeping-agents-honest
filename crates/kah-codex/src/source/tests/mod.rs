use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use super::{provider_item_references_checkout, text_references_checkout};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "driftctl-path-scan-{}-{}-{name}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).expect("create path scanner fixture");
    directory
}

#[test]
fn detects_shell_escaped_spaces_and_case_variants() {
    let parent = directory("case-and-spaces");
    let repository = parent.join("Mixed Case Repository");
    fs::create_dir(&repository).expect("create repository fixture");
    let canonical = repository.canonicalize().expect("canonical repository");
    let escaped = canonical.to_string_lossy().replace(' ', "\\ ");
    assert!(text_references_checkout(
        &format!("sed -n 1p {escaped}/README.md"),
        &canonical
    ));
    assert!(text_references_checkout(
        &format!(
            "open \"{}/README.md\"",
            canonical.to_string_lossy().to_uppercase()
        ),
        &canonical
    ));
    fs::remove_dir_all(parent).expect("remove path scanner fixture");
}

#[test]
fn ignores_only_the_provider_items_structural_cwd() {
    let parent = directory("structural-cwd");
    let repository = parent.join("repository");
    fs::create_dir(&repository).expect("create repository fixture");
    let canonical = repository.canonicalize().expect("canonical repository");
    let structural_only = json!({
        "type":"commandExecution",
        "command":"pwd",
        "commandActions":[],
        "cwd":canonical,
        "status":"completed"
    });
    assert!(!provider_item_references_checkout(
        &structural_only,
        &canonical
    ));
    let nested_tool_path = json!({
        "type":"commandExecution",
        "command":"pwd",
        "commandActions":[{"cwd":canonical.join("README.md")}],
        "cwd":canonical,
        "status":"completed"
    });
    assert!(provider_item_references_checkout(
        &nested_tool_path,
        &canonical
    ));
    fs::remove_dir_all(parent).expect("remove path scanner fixture");
}

#[cfg(unix)]
#[test]
fn detects_symlink_aliases() {
    use std::os::unix::fs::symlink;

    let parent = directory("symlink");
    let repository = parent.join("repository");
    fs::create_dir(&repository).expect("create repository fixture");
    fs::write(repository.join("README.md"), "fixture\n").expect("write fixture");
    let alias = parent.join("alias");
    symlink(&repository, &alias).expect("create repository alias");
    let canonical = repository.canonicalize().expect("canonical repository");
    assert!(text_references_checkout(
        &format!("cat {}/README.md", alias.display()),
        &canonical
    ));
    fs::remove_dir_all(parent).expect("remove path scanner fixture");
}

mod intake;
