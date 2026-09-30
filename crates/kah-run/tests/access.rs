use kah_run::access::share_tree;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
};

#[test]
fn sandbox_access_inherits_operator_rights_without_following_links() {
    let root = std::env::temp_dir().join(format!("kah-run-access-{}", std::process::id()));
    fs::create_dir_all(root.join("tree/nested")).unwrap();
    fs::write(root.join("private"), "private").unwrap();
    fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("tree/nested/file"), "project").unwrap();
    symlink(root.join("private"), root.join("tree/link")).unwrap();
    share_tree(&root.join("tree"), 42420, true).unwrap();
    let acl = Command::new("getfacl")
        .args(["-cpn"])
        .arg(root.join("tree/nested"))
        .output()
        .unwrap();
    assert!(acl.status.success());
    let acl = String::from_utf8(acl.stdout).unwrap();
    assert!(acl.contains("user:42420:rwx"), "{acl}");
    assert!(acl.contains("default:user:1000:rwx"), "{acl}");
    assert_eq!(
        fs::metadata(root.join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(share_tree(&root.join("tree/link"), 42420, true).is_err());
    fs::remove_dir_all(root).unwrap();
}
