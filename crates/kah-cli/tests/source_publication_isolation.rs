//! Security boundary: sandbox evidence is read-only;
//! fixture/publication ACLs must never reach private evidence through an inode.
use kah_cli::observer_review::source_store;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    process::Command,
};
// Requires getfacl and ACL-capable temporary storage.
fn acl(path: &Path) -> String {
    let output = Command::new("/usr/bin/getfacl")
        .args(["-cpn"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn synthetic_uid() -> u32 {
    if kah_sandbox::current_uid().unwrap() == 42420 {
        42421
    } else {
        42420
    }
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn publishing_and_republishing_evidence_never_share_private_inodes_or_acls() {
    let uid = kah_run::access::service_uid().unwrap();
    let root = std::env::temp_dir().join(format!(
        "source-publication-isolation-{}",
        std::process::id()
    ));
    let private = root.join("private");
    let public = root.join("observer");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &public).unwrap();
    fs::create_dir(public.join("outbox")).unwrap();
    let (input, secret, digest) = catalogue_fixture(&private, &public);
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
    let before = acl(&secret);
    fs::create_dir(input.join("sources")).unwrap();
    let alias = input.join("sources").join(&digest);
    // A legacy alias reproduces the installed migration boundary.
    fs::hard_link(&secret, &alias).unwrap();
    for _ in 0..2 {
        source_store::share(&private, &input).unwrap();
        kah_cli::observer_review::workspace::share_access(&public, uid).unwrap();
        assert_ne!(
            fs::metadata(&secret).unwrap().ino(),
            fs::metadata(&alias).unwrap().ino(),
            "sandbox source must be an independent inode"
        );
        assert_eq!(
            acl(&secret),
            before,
            "publishing may not grant any private-store ACL"
        );
        assert!(acl(&alias).contains(&format!("user:{uid}:r--")));
        assert!(!acl(&alias).contains(&format!("user:{uid}:rw")));
        assert!(acl(&public.join("outbox")).contains(&format!("user:{uid}:rwx")));
        assert!(!input.join("sources/unreferenced-record").exists());
    }
    fs::write(&alias, b"fixture changed its copy").unwrap();
    assert_eq!(fs::read(&secret).unwrap(), b"whole original evidence");
    assert_eq!(fs::metadata(&secret).unwrap().nlink(), 1);
    source_store::finish_source_view(&public, "current").unwrap();
    assert!(!input.join("sources").exists());
    assert_eq!(
        source_store::load_text(&input.join("record.txt")).unwrap(),
        "whole original evidence"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_only_publication_replaces_legacy_writable_default_acl() {
    let uid = synthetic_uid();
    let root = std::env::temp_dir().join(format!("source-readonly-default-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    kah_run::access::share_tree(&root, uid, true).unwrap();
    kah_run::access::share_tree(&root, uid, false).unwrap();
    let record = root.join("new-record");
    fs::write(&record, b"whole evidence").unwrap();
    let permissions = acl(&record);
    assert!(permissions.contains(&format!("user:{uid}:r-x")));
    assert!(!permissions.contains(&format!("user:{uid}:rw")));
    assert!(acl(&root).contains(&format!("default:user:{uid}:r-x")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_link_retirement_preserves_private_bytes_without_copying() {
    let uid = synthetic_uid();
    let root = std::env::temp_dir().join(format!("source-link-retirement-{}", std::process::id()));
    let private = root.join("private");
    let workspace = root.join("observer");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &workspace).unwrap();
    let source = private.join("sources/legacy");
    let alias = workspace.join("sources/legacy");
    fs::write(&source, b"complete preserved record").unwrap();
    fs::hard_link(&source, &alias).unwrap();
    kah_run::access::share_tree(&workspace, uid, true).unwrap();
    assert!(acl(&source).contains(&format!("user:{uid}:rw")));
    let result = source_store::retire_shared_sources(&private, &workspace).unwrap();
    assert_eq!(result["retired"], 1);
    assert_eq!(result["copied_bytes"], 0);
    kah_run::access::revoke_tree(&private, uid).unwrap();
    assert!(!acl(&source).contains(&format!("user:{uid}:")));
    assert_eq!(fs::metadata(&source).unwrap().nlink(), 1);
    assert_eq!(fs::read(&source).unwrap(), b"complete preserved record");
    assert!(!alias.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn a_catalogue_view_recovers_nested_json_and_segmented_text_without_unrelated_sources() {
    let uid = kah_run::access::service_uid().unwrap();
    let root = std::env::temp_dir().join(format!("source-view-graph-{}", std::process::id()));
    let private = root.join("private");
    let workspace = root.join("observer");
    let input = workspace.join("inputs/current");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    let original = serde_json::json!({"state":{"rows":[{"encoding":"plain","text":"Whole unicode record 🦀.\n".repeat(12000)}]}});
    source_store::save(&input, "request.json", &original).unwrap();
    fs::write(private.join("sources/unrelated"), b"unrelated old snapshot").unwrap();
    fs::write(
        input.join("context-input.json"),
        serde_json::json!({"full_request":input.join("request.json"),"sources":[]}).to_string(),
    )
    .unwrap();
    source_store::share(&private, &input).unwrap();
    assert!(!input.join("sources/unrelated").exists());
    assert_eq!(
        source_store::read::<serde_json::Value>(&input.join("request.json")).unwrap(),
        original
    );
    for entry in fs::read_dir(input.join("sources")).unwrap() {
        let entry = entry.unwrap();
        let original = private.join("sources").join(entry.file_name());
        assert_ne!(
            entry.metadata().unwrap().ino(),
            fs::metadata(original).unwrap().ino()
        );
        assert!(!acl(&entry.path()).contains(&format!("user:{uid}:rw")));
    }
    source_store::finish_source_view(&workspace, "current").unwrap();
    assert!(!input.join("sources").exists());
    assert_eq!(
        source_store::read::<serde_json::Value>(&input.join("request.json")).unwrap(),
        original
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn publication_revokes_write_access_of_the_real_service_principal() {
    // source_store::share resolves the service principal itself, so the
    // check must name that principal, never the neutral fixture id.
    let uid = kah_run::access::service_uid().unwrap();
    assert_ne!(uid, synthetic_uid());
    let root =
        std::env::temp_dir().join(format!("source-publication-write-{}", std::process::id()));
    let private = root.join("private");
    let public = root.join("observer");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &public).unwrap();
    let (input, _, _) = catalogue_fixture(&private, &public);
    // A writable grant to the real principal, as a legacy publication left it.
    kah_run::access::share_tree(&input, uid, true).unwrap();
    let writable = format!("user:{uid}:rw");
    assert!(acl(&input.join("record.txt")).contains(&writable));
    source_store::share(&private, &input).unwrap();
    let mut paths = vec![
        input.clone(),
        input.join("record.txt"),
        input.join("sources"),
    ];
    for entry in fs::read_dir(input.join("sources")).unwrap() {
        paths.push(entry.unwrap().path());
    }
    assert!(paths.len() > 3, "the catalogue published its source");
    for path in paths {
        let permissions = acl(&path);
        assert!(
            !permissions.contains(&writable),
            "{}: {permissions}",
            path.display()
        );
        assert!(
            permissions.contains(&format!("user:{uid}:r")),
            "{permissions}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

fn catalogue_fixture(
    private: &Path,
    public: &Path,
) -> (std::path::PathBuf, std::path::PathBuf, String) {
    let input = public.join("inputs/current");
    fs::create_dir_all(&input).unwrap();
    source_store::text(
        private,
        &input.join("record.txt"),
        "whole original evidence",
    )
    .unwrap();
    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(input.join("record.txt")).unwrap()).unwrap();
    let digest = envelope["$kah_source"].as_str().unwrap().to_owned();
    let secret = private.join("sources").join(&digest);
    fs::write(
        private.join("sources/unreferenced-record"),
        b"not in this request catalogue",
    )
    .unwrap();
    fs::write(
        input.join("context-input.json"),
        serde_json::json!({"sources":[{"path":input.join("record.txt"),"kind":"ledger"}]})
            .to_string(),
    )
    .unwrap();
    (input, secret, digest)
}

#[test]
fn repair_retires_all_historical_input_aliases_without_copying_the_store() {
    let root = std::env::temp_dir().join(format!("historical-aliases-{}", std::process::id()));
    let private = root.join("private");
    let public = root.join("observer");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &public).unwrap();
    let (_, original, _) = catalogue_fixture(&private, &public);
    let inputs = public.join("inputs/old");
    fs::create_dir_all(&inputs).unwrap();
    for name in ["intent.json", "transcript.crashed.tmp"] {
        fs::hard_link(&original, inputs.join(name)).unwrap();
    }
    let result = source_store::access_repair::retire_inputs(&private, &public).unwrap();
    assert_eq!(result["retired"], 2);
    assert_eq!(result["copied_bytes"], 0);
    assert!(!inputs.join("intent.json").exists());
    assert!(!inputs.join("transcript.crashed.tmp").exists());
    assert_eq!(fs::read(&original).unwrap(), b"whole original evidence");
    assert_eq!(fs::metadata(&original).unwrap().nlink(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn importing_a_sandbox_report_keeps_bytes_but_not_its_inode_or_acl() {
    let uid = synthetic_uid();
    let root = std::env::temp_dir().join(format!("report-import-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("sandbox.json");
    let target = root.join("private.json");
    fs::write(&source, b"whole sandbox report evidence").unwrap();
    kah_run::access::share_tree(&root, uid, true).unwrap();
    let original_inode = fs::metadata(&source).unwrap().ino();
    source_store::access_repair::private_copy(&source, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), fs::read(&source).unwrap());
    assert_ne!(fs::metadata(&target).unwrap().ino(), original_inode);
    assert!(!acl(&target).contains(&format!("user:{uid}:")));
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    source_store::access_repair::private_copy(&source, &source).unwrap();
    assert_ne!(fs::metadata(&source).unwrap().ino(), original_inode);
    assert!(!acl(&source).contains(&format!("user:{uid}:")));
    assert_eq!(fs::read(&source).unwrap(), b"whole sandbox report evidence");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn importing_a_report_rejects_a_symbolic_link_before_reading_it() {
    let root = std::env::temp_dir().join(format!("report-symlink-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let link = root.join("sandbox.json");
    std::os::unix::fs::symlink(root.join("missing-private-file"), &link).unwrap();
    let error =
        source_store::access_repair::private_copy(&link, &root.join("private.json")).unwrap_err();
    assert!(error.to_string().contains("REVIEW_PRIVATE_COPY_TYPE"));
    assert!(!root.join("private.json").exists());
    fs::remove_dir_all(root).unwrap();
}
