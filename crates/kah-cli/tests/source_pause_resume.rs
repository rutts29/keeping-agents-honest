//! Host-only ACL witness; no agents, provider calls or live-store edits.
//! google/gvisor release-20261005.0 runsc/boot/vfs.go:416-424 uses
//! cache=remote_revalidating for shared directory mounts. Descriptor access
//! belongs to the publication boundary; stale historical gofer attribution
//! is deliberately not asserted by this test.
use kah_cli::observer_review::source_store;
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn paused_request_publication_restores_descriptor_and_view_read_access() {
    let fixture_service_uid = kah_run::access::service_uid().unwrap();
    let root = std::env::temp_dir().join(format!("paused-source-{}", std::process::id()));
    let private = root.join("private");
    let workspace = root.join("observer");
    let input = workspace.join("inputs/current");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &workspace).unwrap();
    fs::create_dir_all(&input).unwrap();
    let descriptor = input.join("option1.txt");
    source_store::text(&private, &descriptor, "Whole authorized evidence.\n").unwrap();
    fs::write(
        input.join("context-input.json"),
        serde_json::json!({
            "sources":[{"path":descriptor,"kind":"literal_first"}]
        })
        .to_string(),
    )
    .unwrap();
    kah_run::access::share_tree(&input, fixture_service_uid, false).unwrap();
    fs::write(workspace.join("observer-paused"), "native provider block").unwrap();
    // Atomic private saves mask inherited ACLs until publication grants read.
    source_store::text(&private, &descriptor, "Whole authorized evidence.\n").unwrap();
    fs::set_permissions(&descriptor, fs::Permissions::from_mode(0o600)).unwrap();
    let original = fs::read(&descriptor).unwrap();
    assert!(acl(&descriptor).contains("#effective:---"));
    fs::remove_file(workspace.join("observer-paused")).unwrap();
    source_store::share(&private, &input).unwrap();
    let after = acl(&descriptor);
    assert!(after.contains(format!("user:{fixture_service_uid}:r-").as_str()), "{after}");
    assert!(!after.contains("#effective:---"), "{after}");
    assert!(!after.contains(format!("user:{fixture_service_uid}:rw").as_str()));
    assert_eq!(fs::read(&descriptor).unwrap(), original);
    assert_eq!(
        source_store::load_text(&descriptor).unwrap(),
        "Whole authorized evidence.\n"
    );
    for entry in fs::read_dir(private.join("sources")).unwrap() {
        assert!(!acl(&entry.unwrap().path()).contains(format!("user:{fixture_service_uid}:").as_str()));
    }
    fs::remove_dir_all(root).unwrap();
}
fn acl(path: &std::path::Path) -> String {
    let output = Command::new("getfacl")
        .args(["-cpn", "--"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
