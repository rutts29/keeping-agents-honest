//! VF14: the private review-id ancestor prevented Docker from statting its bind.
use super::*;
use std::os::unix::fs::symlink;
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn acl(path: &Path) -> String {
    let out = Command::new("getfacl")
        .args(["-cpn"])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

fn git(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=test@invalid",
        ])
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

fn sandbox_candidate(root: &Path) -> Worktree {
    let home = root.join("home");
    kah_core::run_store::ensure_save_directory(&home.join(".local/bin")).unwrap();
    symlink(
        "/home/example/.local/bin/codex",
        home.join(".local/bin/codex"),
    )
    .unwrap();
    let source = root.join("source");
    kah_core::run_store::ensure_save_directory(&source).unwrap();
    git(&source, &["init", "--template="]);
    fs::write(
        source.join("check.cjs"),
        "require('node:assert/strict').equal(6*7,42);console.log('verification ok');\n",
    )
    .unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "fixture"]);
    let parent = root.join("review/interactive-gates/review-vf14");
    kah_core::run_store::ensure_save_directory(&parent).unwrap();
    Worktree::create(&source, &parent.join("candidate"), "run-vf14-acl").unwrap()
}

#[test]
#[ignore = "sudo-free actual gVisor bind/privacy regression; zero native agents and zero Jev"]
fn verifier_bind_is_accessible_but_source_refs_and_sibling_data_are_unreachable() {
    let root = std::env::temp_dir().join(format!("vf14-sandbox-{}", crate::relay_store::nonce()));
    let candidate = sandbox_candidate(&root);
    let home = root.join("home");
    let source = root.join("source");
    let head = candidate.head().unwrap();
    let parent = root.join("review/interactive-gates/review-vf14");
    let sibling = parent.join("sibling");
    kah_core::run_store::ensure_save_directory(&sibling).unwrap();
    fs::write(sibling.join("private"), "SYNTHETIC_PRIVATE_SENTINEL").unwrap();
    let script = format!(
        "node check.cjs && test ! -e '{}' && test ! -e '{}' && ! git tag verifier-must-not-write",
        source.display(),
        sibling.display()
    );
    let (receipt, changed) = execute(
        &home,
        &candidate,
        &head,
        Default::default(),
        &script,
        &parent.join("check.log"),
        || Ok(false),
    )
    .unwrap();
    assert!(receipt.success, "{}", receipt.output);
    assert!(!changed);
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&source, &["tag", "--list"]), "");
    assert_eq!(candidate.head().unwrap(), head);
    assert_eq!(git(&candidate.path, &["tag", "--list"]), "");
    let uid = access::service_uid().unwrap();
    for p in [&root, &parent, &parent.join("candidate")] {
        let value = acl(p);
        assert!(value.contains(&format!("user:{uid}:--x")));
        println!("VERIFIER_ACL {}\n{value}", p.display());
    }
    assert!(!acl(&sibling).contains(&format!("user:{uid}:")));
    let retained: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-verifier-replay/retained-check.json"
    ))
    .unwrap();
    let receipt = serde_json::from_value(retained["receipt"].clone()).unwrap();
    assert!(
        checked_receipt(receipt, Path::new("retained.log"))
            .err()
            .unwrap()
            .to_string()
            .starts_with("RUN_VERIFIER_INFRA_GAP stage=verification")
    );
    println!(
        "VERIFIER_ISOLATION_PASS bind=accessible source_refs=unchanged siblings=private sudo=false native_agents=0 Jev_calls=0"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn private_verifier_ancestors_allow_only_traversal_and_leave_siblings_private() {
    let root = std::env::temp_dir().join(format!("vf14-access-{}", crate::relay_store::nonce()));
    let parent = root.join("review/interactive-gates/review-vf14");
    let bind = parent.join("candidate/verify");
    kah_core::run_store::ensure_save_directory(&bind).unwrap();
    let sibling = parent.join("other-review");
    kah_core::run_store::ensure_save_directory(&sibling).unwrap();
    let uid = access::service_uid().unwrap();
    access::traverse(&root, uid).unwrap();
    let before = acl(&root);
    access_path::prepare(&bind, uid).unwrap();
    for ancestor in bind
        .ancestors()
        .skip(1)
        .take_while(|p| p.starts_with(&root))
    {
        assert!(
            acl(ancestor).contains(&format!("user:{uid}:--x")),
            "{}",
            ancestor.display()
        );
    }
    assert_eq!(acl(&root), before);
    assert!(
        Command::new("setfacl")
            .args(["-m", &format!("u:{uid}:r-x")])
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    let existing_acl = acl(&root);
    let existing_mode = fs::metadata(&root).unwrap().permissions().mode();
    access_path::prepare(&bind, uid).unwrap();
    assert_eq!(
        acl(&root),
        existing_acl,
        "existing named/default ACLs are preserved"
    );
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode(),
        existing_mode
    );
    assert_eq!(
        fs::metadata(&sibling).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(!acl(&sibling).contains(&format!("user:{uid}:")));
    restricted_mask(&root, uid);
    fs::remove_dir_all(root).unwrap();
}

fn restricted_mask(root: &Path, uid: u32) {
    let bind = root.join("restricted-mask/bind");
    kah_core::run_store::ensure_save_directory(&bind).unwrap();
    let parent = bind.parent().unwrap();
    assert!(
        Command::new("setfacl")
            .args(["-m", "u:65533:rwx,m::r--"])
            .arg(parent)
            .status()
            .unwrap()
            .success()
    );
    let before = acl(parent);
    let audit = access_path::prepare(&bind, uid).unwrap();
    let entry = audit
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == serde_json::json!(parent))
        .unwrap();
    assert_eq!(entry["prior_mask"], "r--");
    assert_eq!(entry["prior_acl"], before);
    let after = acl(parent);
    assert!(after.contains("mask::r-x"), "{after}");
    assert!(
        !after.contains("mask::rwx"),
        "hidden write rights must stay masked"
    );
}

#[test]
fn verifier_deadline_covers_a_cold_rust_build_in_gvisor() {
    assert_eq!(DEADLINE, Duration::from_secs(600));
}
