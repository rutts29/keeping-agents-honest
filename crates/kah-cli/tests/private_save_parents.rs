//! Missing review directories must be private; existing traverse ACLs stay intact.
use kah_cli::{observer_review::source_store, unattended_report};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
};

fn acl(path: &Path) -> Vec<u8> {
    let result = Command::new("getfacl")
        .args(["-c", "-p"])
        .arg(path)
        .output()
        .unwrap();
    assert!(result.status.success());
    result.stdout
}
fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}
#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn saves_create_all_missing_private_levels_and_preserve_existing_traverse_acl() {
    let root = std::env::temp_dir().join(format!("save-parent-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(root.join("sources")).unwrap();
    let uid = kah_run::access::service_uid().unwrap();
    assert!(
        Command::new("setfacl")
            .args(["-m", &format!("u:{uid}:--x")])
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    let before_acl = acl(&root);
    let before_mode = mode(&root);
    let value = json!({"private":"whole record"});
    for (kind, path) in [
        ("report", root.join("report/new/leaf")),
        ("source", root.join("source/new/leaf")),
    ] {
        if kind == "report" {
            unattended_report::save(&path, "record.json", &value).unwrap();
        } else {
            source_store::save(&path, "record.json", &value).unwrap();
        }
        for dir in [
            &path,
            path.parent().unwrap(),
            path.parent().unwrap().parent().unwrap(),
        ] {
            assert_eq!(mode(dir), 0o700, "new parent {}", dir.display());
        }
        assert_eq!(mode(&path.join("record.json")), 0o600);
        assert_eq!(
            source_store::read::<serde_json::Value>(&path.join("record.json")).unwrap(),
            value
        );
    }
    assert_eq!(acl(&root), before_acl);
    assert_eq!(mode(&root), before_mode);
    symlink(&root, root.join("alias")).unwrap();
    assert!(unattended_report::save(&root.join("alias/new"), "record.json", &value).is_err());
    fs::write(root.join("not-directory"), "data").unwrap();
    assert!(source_store::save(&root.join("not-directory/new"), "record.json", &value).is_err());
    fs::remove_file(root.join("alias")).unwrap();
    fs::remove_dir_all(root).unwrap();
}
